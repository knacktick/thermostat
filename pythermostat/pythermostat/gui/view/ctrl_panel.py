from functools import partial
from PyQt6.QtCore import pyqtSignal, QObject, QSignalBlocker, pyqtSlot
import pyqtgraph.parametertree.parameterTypes as pTypes
from pyqtgraph.parametertree import (
    Parameter,
    registerParameterType,
)
from qasync import asyncSlot
from pythermostat.autotune import PIDAutotuneState


class MutexParameter(pTypes.ListParameter):
    """
    Mutually exclusive parameter where only one of its children is visible at a time, list selectable.

    The ordering of the list items determines which children will be visible.
    """

    def __init__(self, **opts):
        super().__init__(**opts)

        self.sigValueChanged.connect(self.show_chosen_child)
        self.sigValueChanged.emit(self, self.opts["value"])

    def _get_param_from_value(self, value):
        if isinstance(self.opts["limits"], dict):
            values_list = list(self.opts["limits"].values())
        else:
            values_list = self.opts["limits"]

        return self.children()[values_list.index(value)]

    @pyqtSlot(object, object)
    def show_chosen_child(self, value):
        for param in self.children():
            param.hide()

        child_to_show = self._get_param_from_value(value.value())
        child_to_show.show()

        if child_to_show.opts.get("triggerOnShow", None):
            child_to_show.sigValueChanged.emit(child_to_show, child_to_show.value())


registerParameterType("mutex", MutexParameter)


class CtrlPanel(QObject):
    def __init__(
        self,
        thermostat,
        autotuners,
        info_box,
        trees_ui,
        param_tree,
        parent=None,
    ):
        super().__init__(parent)

        self.thermostat = thermostat
        self.autotuners = autotuners
        self.info_box = info_box
        self.trees_ui = trees_ui
        self.NUM_CHANNELS = len(trees_ui)

        self.THERMOSTAT_PARAMETERS = [param_tree for i in range(self.NUM_CHANNELS)]

        self.params = [
            Parameter.create(
                name=f"Thermostat Channel {ch} Parameters",
                type="group",
                value=ch,
                children=self.THERMOSTAT_PARAMETERS[ch],
            )
            for ch in range(self.NUM_CHANNELS)
        ]

        for i, param in enumerate(self.params):
            param.channel = i

        for i, tree in enumerate(self.trees_ui):
            tree.setHeaderHidden(True)
            tree.setParameters(self.params[i], showTop=False)
            self.params[i].setValue = self._setValue
            self.params[i].sigTreeStateChanged.connect(self.send_command)

            self.params[i].child("Save to flash").sigActivated.connect(
                partial(self.save_settings, i)
            )
            self.params[i].child("Load from flash").sigActivated.connect(
                partial(self.load_settings, i)
            )
            self.params[i].child(
                "PID Config", "PID Auto Tune", "Run"
            ).sigActivated.connect(partial(self.pid_auto_tune_request, i))

        self.thermostat.pid_update.connect(self.update_pid)
        self.thermostat.report_update.connect(self.update_report)
        self.thermostat.thermistor_update.connect(self.update_thermistor)
        self.thermostat.output_update.connect(self.update_output)
        self.thermostat.postfilter_update.connect(self.update_postfilter)
        self.autotuners.autotune_state_changed.connect(self.update_pid_autotune)

    def _setValue(self, value, blockSignal=None):
        """
        Implement 'lock' mechanism for Parameter Type

        Modified from the source
        """
        try:
            if blockSignal is not None:
                self.sigValueChanged.disconnect(blockSignal)
            value = self._interpretValue(value)
            if fn.eq(self.opts["value"], value):
                return value

            if "lock" in self.opts.keys():
                if self.opts["lock"]:
                    return value
            self.opts["value"] = value
            self.sigValueChanged.emit(
                self, value
            )  # value might change after signal is received by tree item
        finally:
            if blockSignal is not None:
                self.sigValueChanged.connect(blockSignal)

        return self.opts["value"]

    def change_params_title(self, channel, path, title):
        self.params[channel].child(*path).setOpts(title=title)

    @asyncSlot(object, object)
    async def send_command(self, param, changes):
        """Translates parameter tree changes into thermostat set_param calls"""
        ch = param.channel

        for inner_param, change, data in changes:
            if change == "value":
                new_value = data
                if "thermostat:set_param" in inner_param.opts:
                    if inner_param.opts.get("suffix", None) == "mA":
                        new_value /= 1000  # Given in mA

                    thermostat_param = inner_param.opts["thermostat:set_param"]

                    # Handle thermostat command irregularities
                    match inner_param.name(), new_value:
                        case "Postfilter Rate", None:
                            thermostat_param = thermostat_param.copy()
                            thermostat_param["field"] = "off"
                            new_value = ""
                        case "Control Method", "Constant Current":
                            return
                        case "Control Method", "Temperature PID":
                            new_value = ""

                    inner_param.setOpts(lock=True)
                    await self.thermostat.set_param(
                        channel=ch, value=new_value, **thermostat_param
                    )
                    inner_param.setOpts(lock=False)

                if "pid_autotune" in inner_param.opts:
                    auto_tuner_param = inner_param.opts["pid_autotune"]
                    self.autotuners.set_params(auto_tuner_param, ch, new_value)

    @pyqtSlot(list)
    def update_pid(self, pid_settings):
        for settings in pid_settings:
            channel = settings["channel"]
            with QSignalBlocker(self.params[channel]):
                self.params[channel].child("PID Config", "Kp").setValue(
                    settings["parameters"]["kp"]
                )
                self.params[channel].child("PID Config", "Ki").setValue(
                    settings["parameters"]["ki"]
                )
                self.params[channel].child("PID Config", "Kd").setValue(
                    settings["parameters"]["kd"]
                )
                self.params[channel].child(
                    "PID Config", "PID Output Clamping", "Minimum"
                ).setValue(settings["parameters"]["output_min"] * 1000)
                self.params[channel].child(
                    "PID Config", "PID Output Clamping", "Maximum"
                ).setValue(settings["parameters"]["output_max"] * 1000)
                self.params[channel].child(
                    "Output Config", "Control Method", "Set Temperature"
                ).setValue(settings["target"])

    @pyqtSlot(list)
    def update_report(self, report_data):
        for settings in report_data:
            channel = settings["channel"]
            with QSignalBlocker(self.params[channel]):
                self.params[channel].child("Output Config", "Control Method").setValue(
                    "Temperature PID" if settings["pid_engaged"] else "Constant Current"
                )
                self.params[channel].child(
                    "Output Config", "Control Method", "Set Current"
                ).setValue(settings["i_set"] * 1000)
                if settings["temperature"] is not None:
                    self.params[channel].child("Temperature").setValue(
                        settings["temperature"]
                    )
                    if settings["tec_i"] is not None:
                        self.params[channel].child("Current through TEC").setValue(
                            settings["tec_i"] * 1000
                        )

    @pyqtSlot(list)
    def update_thermistor(self, sh_data):
        for sh_param in sh_data:
            channel = sh_param["channel"]
            with QSignalBlocker(self.params[channel]):
                self.params[channel].child("Thermistor Config", "T₀").setValue(
                    sh_param["params"]["t0"] - 273.15
                )
                self.params[channel].child("Thermistor Config", "R₀").setValue(
                    sh_param["params"]["r0"]
                )
                self.params[channel].child("Thermistor Config", "B").setValue(
                    sh_param["params"]["b"]
                )

    @pyqtSlot(list)
    def update_output(self, output_data):
        for output_params in output_data:
            channel = output_params["channel"]
            with QSignalBlocker(self.params[channel]):
                self.params[channel].child(
                    "Output Config", "Limits", "Max Voltage Difference"
                ).setValue(output_params["max_v"])
                self.params[channel].child(
                    "Output Config", "Limits", "Max Cooling Current"
                ).setValue(output_params["max_i_pos"] * 1000)
                self.params[channel].child(
                    "Output Config", "Limits", "Max Heating Current"
                ).setValue(output_params["max_i_neg"] * 1000)

    @pyqtSlot(list)
    def update_postfilter(self, postfilter_data):
        for postfilter_params in postfilter_data:
            channel = postfilter_params["channel"]
            with QSignalBlocker(self.params[channel]):
                self.params[channel].child(
                    "Thermistor Config", "Postfilter Rate"
                ).setValue(postfilter_params["rate"])

    def update_pid_autotune(self, ch, state):
        match state:
            case PIDAutotuneState.OFF:
                self.change_params_title(
                    ch, ("PID Config", "PID Auto Tune", "Run"), "Run"
                )
            case (
                PIDAutotuneState.READY
                | PIDAutotuneState.RELAY_STEP_UP
                | PIDAutotuneState.RELAY_STEP_DOWN
            ):
                self.change_params_title(
                    ch, ("PID Config", "PID Auto Tune", "Run"), "Stop"
                )
            case PIDAutotuneState.SUCCEEDED:
                self.info_box.display_info_box(
                    "PID Autotune Success",
                    f"Channel {ch} PID Config has been loaded to Thermostat. Regulating temperature.",
                )
            case PIDAutotuneState.FAILED:
                self.info_box.display_info_box(
                    "PID Autotune Failed",
                    f"Channel {ch} PID Autotune has failed.",
                )

    @asyncSlot(int)
    async def load_settings(self, ch):
        await self.thermostat.load_cfg(ch)

        self.info_box.display_info_box(
            f"Channel {ch} settings loaded",
            f"Channel {ch} settings has been loaded from flash.",
        )

    @asyncSlot(int)
    async def save_settings(self, ch):
        await self.thermostat.save_cfg(ch)

        self.info_box.display_info_box(
            f"Channel {ch} settings saved",
            f"Channel {ch} settings has been saved to flash.\n"
            "It will be loaded on Thermostat reset, or when settings are explicitly loaded.",
        )

    @asyncSlot()
    async def pid_auto_tune_request(self, ch=0):
        match self.autotuners.get_state(ch):
            case PIDAutotuneState.OFF | PIDAutotuneState.FAILED:
                self.autotuners.load_params_and_set_ready(ch)

            case (
                PIDAutotuneState.READY
                | PIDAutotuneState.RELAY_STEP_UP
                | PIDAutotuneState.RELAY_STEP_DOWN
            ):
                await self.autotuners.stop_pid_from_running(ch)
