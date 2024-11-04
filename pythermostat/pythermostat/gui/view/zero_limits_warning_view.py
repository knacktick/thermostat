from PyQt6.QtCore import pyqtSlot, QObject
from PyQt6 import QtWidgets, QtGui


class ZeroLimitsWarningView(QObject):
    def __init__(self, thermostat, style, limit_warning):
        super().__init__()
        self._thermostat = thermostat
        self._thermostat.output_update.connect(self.set_limits_warning)
        self._lbl = limit_warning
        self._style = style

    @pyqtSlot(list)
    def set_limits_warning(self, output_data: list):
        channels_zeroed_limits = [set() for i in range(self._thermostat.NUM_CHANNELS)]

        for output_params in output_data:
            channel = output_params["channel"]
            for limit in "max_i_pos", "max_i_neg", "max_v":
                if output_params[limit] == 0.0:
                    channels_zeroed_limits[channel].add(limit)

        channel_disabled = [False, False]
        report_str = "The following output limit(s) are set to zero:\n"
        for ch, zeroed_limits in enumerate(channels_zeroed_limits):
            if {"max_i_pos", "max_i_neg"}.issubset(zeroed_limits):
                report_str += "Max Cooling Current, Max Heating Current"
                channel_disabled[ch] = True

            if "max_v" in zeroed_limits:
                if channel_disabled[ch]:
                    report_str += ", "
                report_str += "Max Voltage Difference"
                channel_disabled[ch] = True

            if channel_disabled[ch]:
                report_str += f" for Channel {ch}\n"

        report_str += (
            "\nThese limit(s) are restricting the channel(s) from producing current."
        )

        if True in channel_disabled:
            pixmapi = getattr(QtWidgets.QStyle.StandardPixmap, "SP_MessageBoxWarning")
            icon = self._style.standardIcon(pixmapi)
            self._lbl.setPixmap(icon.pixmap(16, 16))
            self._lbl.setToolTip(report_str)
        else:
            self._lbl.setPixmap(QtGui.QPixmap())
            self._lbl.setToolTip(None)
