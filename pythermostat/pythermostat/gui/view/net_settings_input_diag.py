from PyQt6 import QtWidgets
from PyQt6.QtWidgets import QAbstractButton
from PyQt6.QtCore import pyqtSignal, pyqtSlot


class NetSettingsInputDiag(QtWidgets.QInputDialog):
    set_ipv4_act = pyqtSignal(str)

    def __init__(self, current_ipv4_settings):
        super().__init__()
        self.setWindowTitle("Network Settings")
        self.setLabelText(
            "Set the Thermostat's IPv4 address, netmask and gateway (optional)"
        )
        self.setTextValue(current_ipv4_settings)
        self._new_ipv4 = ""

        @pyqtSlot(str)
        def set_ipv4(ipv4_settings):
            self._new_ipv4 = ipv4_settings

            sure = QtWidgets.QMessageBox(self)
            sure.setWindowTitle("Set network?")
            sure.setText(
                f"Setting this as network and disconnecting:<br>{ipv4_settings}"
            )

            sure.buttonClicked.connect(self._emit_sig)
            sure.show()

        self.textValueSelected.connect(set_ipv4)
        self.show()

    @pyqtSlot(QAbstractButton)
    def _emit_sig(self, _):
        self.set_ipv4_act.emit(self._new_ipv4)
