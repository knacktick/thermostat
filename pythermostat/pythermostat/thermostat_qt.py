"""GUI for the Sinara 8451 Thermostat"""

import asyncio
import logging
import argparse
import importlib.resources
from PyQt6 import QtWidgets, QtGui, uic
from PyQt6.QtCore import pyqtSlot
import qasync
from qasync import asyncSlot, asyncClose
from pythermostat.autotune import PIDAutotuneState
from pythermostat.gui.model.thermostat import Thermostat, ThermostatConnectionState
from pythermostat.gui.model.pid_autotuner import PIDAutoTuner
from pythermostat.gui.view.info_box import InfoBox
from pythermostat.gui.view.menus import PlotOptionsMenu, ThermostatSettingsMenu, ConnectionDetailsMenu
from pythermostat.gui.view.live_plot_view import LiveDataPlotter
from pythermostat.gui.view.zero_limits_warning_view import ZeroLimitsWarningView


def get_argparser():
    parser = argparse.ArgumentParser(description="Thermostat Control Panel")

    parser.add_argument(
        "--connect",
        default=None,
        action="store_true",
        help="Automatically connect to the specified Thermostat in host:port format",
    )
    parser.add_argument("host", metavar="HOST", default=None, nargs="?")
    parser.add_argument("port", metavar="PORT", default=None, nargs="?")
    parser.add_argument(
        "-l",
        "--log",
        dest="logLevel",
        choices=["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"],
        help="Set the logging level",
    )

    return parser


class MainWindow(QtWidgets.QMainWindow):
    NUM_CHANNELS = 2

    def __init__(self):
        super().__init__()

        ui_file_path = importlib.resources.files("pythermostat.gui.view").joinpath("MainWindow.ui")
        uic.loadUi(ui_file_path, self)

        self._info_box = InfoBox()

        # Models
        self._thermostat = Thermostat(self, self.report_refresh_spin.value())
        self._connecting_task = None
        self._thermostat.connection_state_update.connect(
            self._on_connection_state_changed
        )

        self._autotuners = PIDAutoTuner(self, self._thermostat, 2)
        self._autotuners.autotune_state_changed.connect(
            self._on_pid_autotune_state_changed
        )

        # Handlers for disconnections
        async def autotune_disconnect():
            for ch in range(self.NUM_CHANNELS):
                if self._autotuners.get_state(ch) != PIDAutotuneState.OFF:
                    await self._autotuners.stop_pid_from_running(ch)

        self._thermostat.disconnect_cb = autotune_disconnect

        @pyqtSlot()
        def handle_connection_error():
            self._info_box.display_info_box(
                "Connection Error", "Thermostat connection lost. Is it unplugged?"
            )

        self._thermostat.connection_error.connect(handle_connection_error)

        # Graphs
        self._channel_graphs = LiveDataPlotter(
            self._thermostat,
            [
                [getattr(self, f"ch{ch}_t_graph"), getattr(self, f"ch{ch}_i_graph")]
                for ch in range(self.NUM_CHANNELS)
            ],
        )

        # Bottom bar menus
        self.connection_details_menu = ConnectionDetailsMenu(
            self._thermostat, self.connect_btn
        )
        self.connect_btn.setMenu(self.connection_details_menu)

        self._thermostat_settings_menu = ThermostatSettingsMenu(
            self._thermostat, self._info_box, self.style()
        )
        self.thermostat_settings.setMenu(self._thermostat_settings_menu)

        self._plot_options_menu = PlotOptionsMenu(self._channel_graphs)
        self.plot_settings.setMenu(self._plot_options_menu)

        # Status line
        self._zero_limits_warning_view = ZeroLimitsWarningView(
            self._thermostat, self.style(), self.limits_warning
        )
        self.loading_spinner.hide()

        self.report_apply_btn.clicked.connect(
            lambda: self._thermostat.set_update_s(self.report_refresh_spin.value())
        )

    @asyncClose
    async def closeEvent(self, _event):
        try:
            await self._thermostat.end_session()
            self._thermostat.connection_state = ThermostatConnectionState.DISCONNECTED
        except:
            pass

    @pyqtSlot(ThermostatConnectionState)
    def _on_connection_state_changed(self, state):
        self.graph_group.setEnabled(state == ThermostatConnectionState.CONNECTED)
        self.thermostat_settings.setEnabled(
            state == ThermostatConnectionState.CONNECTED
        )
        self.report_group.setEnabled(state == ThermostatConnectionState.CONNECTED)

        match state:
            case ThermostatConnectionState.CONNECTED:
                self.connect_btn.setText("Disconnect")
                self.status_lbl.setText(
                    "Connected to Thermostat v"
                    f"{self._thermostat.hw_rev['rev']['major']}."
                    f"{self._thermostat.hw_rev['rev']['minor']}"
                )

            case ThermostatConnectionState.CONNECTING:
                self.connect_btn.setText("Stop")
                self.status_lbl.setText("Connecting...")

            case ThermostatConnectionState.DISCONNECTED:
                self.connect_btn.setText("Connect")
                self.status_lbl.setText("Disconnected")

    @pyqtSlot(int, PIDAutotuneState)
    def _on_pid_autotune_state_changed(self, _ch, _state):
        autotuning_channels = []
        for ch in range(self.NUM_CHANNELS):
            if self._autotuners.get_state(ch) in {
                PIDAutotuneState.READY,
                PIDAutotuneState.RELAY_STEP_UP,
                PIDAutotuneState.RELAY_STEP_DOWN,
            }:
                autotuning_channels.append(ch)

        if len(autotuning_channels) == 0:
            self.background_task_lbl.setText("Ready.")
            self.loading_spinner.hide()
            self.loading_spinner.stop()
        else:
            self.background_task_lbl.setText(
                f"Autotuning channel {autotuning_channels}..."
            )
            self.loading_spinner.start()
            self.loading_spinner.show()

    @asyncSlot()
    async def on_connect_btn_clicked(self):
        match self._thermostat.connection_state:
            case ThermostatConnectionState.DISCONNECTED:
                self._connecting_task = asyncio.current_task()
                self._thermostat.connection_state = ThermostatConnectionState.CONNECTING
                await self._thermostat.start_session(
                    host=self.connection_details_menu.host_set_line.text(),
                    port=self.connection_details_menu.port_set_spin.value(),
                )
                self._connecting_task = None
                self._thermostat.connection_state = ThermostatConnectionState.CONNECTED
                self._thermostat.start_watching()

            case ThermostatConnectionState.CONNECTING:
                self._connecting_task.cancel()
                self._connecting_task = None
                await self._thermostat.end_session()
                self._thermostat.connection_state = (
                    ThermostatConnectionState.DISCONNECTED
                )

            case ThermostatConnectionState.CONNECTED:
                await self._thermostat.end_session()
                self._thermostat.connection_state = (
                    ThermostatConnectionState.DISCONNECTED
                )


async def coro_main():
    args = get_argparser().parse_args()
    if args.logLevel:
        logging.basicConfig(level=getattr(logging, args.logLevel))

    app_quit_event = asyncio.Event()

    app = QtWidgets.QApplication.instance()
    app.aboutToQuit.connect(app_quit_event.set)

    main_window = MainWindow()
    main_window.show()

    if args.connect:
        if args.host:
            main_window.connection_details_menu.host_set_line.setText(args.host)
        if args.port:
            main_window.connection_details_menu.port_set_spin.setValue(int(args.port))
        main_window.connect_btn.click()

    await app_quit_event.wait()


def main():
    qasync.run(coro_main())


if __name__ == "__main__":
    main()
