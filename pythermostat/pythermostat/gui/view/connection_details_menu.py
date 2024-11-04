from PyQt6 import QtWidgets, QtCore
from PyQt6.QtCore import pyqtSlot
from pythermostat.gui.model.thermostat import ThermostatConnectionState


class ConnectionDetailsMenu(QtWidgets.QMenu):
    def __init__(self, thermostat, connect_btn):
        super().__init__()
        self._thermostat = thermostat
        self._connect_btn = connect_btn
        self._thermostat.connection_state_update.connect(
            self.thermostat_state_change_handler
        )

        self._setup_menu_items()

    @pyqtSlot(ThermostatConnectionState)
    def thermostat_state_change_handler(self, state):
        self.host_set_line.setEnabled(state == ThermostatConnectionState.DISCONNECTED)
        self.port_set_spin.setEnabled(state == ThermostatConnectionState.DISCONNECTED)

    def _setup_menu_items(self):
        # Sets Thermostat host/IP
        self.host_set_line = QtWidgets.QLineEdit()
        self.host_set_line.setMinimumWidth(160)
        self.host_set_line.setMaximumWidth(160)
        self.host_set_line.setMaxLength(15)
        self.host_set_line.setClearButtonEnabled(True)
        self.host_set_line.setText("192.168.1.26")
        self.host_set_line.setPlaceholderText("IP for the Thermostat")

        def connect_on_enter_press():
            self._connect_btn.click()
            self.hide()

        self.host_set_line.returnPressed.connect(connect_on_enter_press)

        host = QtWidgets.QWidgetAction(self)
        host.setDefaultWidget(self.host_set_line)
        self.addAction(host)

        # Sets Thermostat port
        self.port_set_spin = QtWidgets.QSpinBox()
        self.port_set_spin.setMinimumWidth(70)
        self.port_set_spin.setMaximumWidth(70)
        self.port_set_spin.setMaximum(65535)
        self.port_set_spin.setValue(23)

        def connect_only_if_enter_pressed():
            if (
                not self.port_set_spin.hasFocus()
            ):  # Don't connect if the spinbox only lost focus
                return
            connect_on_enter_press()

        self.port_set_spin.editingFinished.connect(connect_only_if_enter_pressed)

        port = QtWidgets.QWidgetAction(self)
        port.setDefaultWidget(self.port_set_spin)
        self.addAction(port)

        # Exits GUI
        exit_button = QtWidgets.QPushButton()
        exit_button.setText("Exit GUI")
        exit_button.pressed.connect(QtWidgets.QApplication.instance().quit)

        exit_action = QtWidgets.QWidgetAction(exit_button)
        exit_action.setDefaultWidget(exit_button)
        self.addAction(exit_action)
