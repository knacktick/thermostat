from PyQt6 import QtWidgets, QtCore, QtGui
from PyQt6.QtCore import pyqtSlot, QSignalBlocker
from qasync import asyncSlot
from pythermostat.gui.model.thermostat import ThermostatConnectionState
from pythermostat.gui.view.net_settings_input_diag import NetSettingsInputDiag


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


class PlotOptionsMenu(QtWidgets.QMenu):
    def __init__(self, channel_graphs, max_samples=1000):
        super().__init__()

        # Clears plots for both graphs in all channels
        clear_graphs = QtGui.QAction("Clear graphs", self)
        clear_graphs.triggered.connect(channel_graphs.clear_graphs)
        self.addAction(clear_graphs)

        # Set maximum samples in graphs
        samples_spinbox = QtWidgets.QSpinBox()
        samples_spinbox.setRange(2, 100000)
        samples_spinbox.setSuffix(" samples")
        samples_spinbox.setValue(max_samples)
        samples_spinbox.valueChanged.connect(channel_graphs.set_max_samples)

        limit_samples = QtWidgets.QWidgetAction(self)
        limit_samples.setDefaultWidget(samples_spinbox)
        self.addAction(limit_samples)


class ThermostatSettingsMenu(QtWidgets.QMenu):
    def __init__(self, thermostat, info_box, style):
        super().__init__()
        self._thermostat = thermostat
        self._info_box = info_box
        self._style = style

        self.hw_rev_data = {}
        self._thermostat.hw_rev_update.connect(self.hw_rev)
        self._thermostat.connection_state_update.connect(
            self.thermostat_state_change_handler
        )

        self._setup_menu_items()

    @pyqtSlot("QVariantMap")
    def fan_update(self, fan_settings):
        if fan_settings is None:
            return
        with QSignalBlocker(self.fan_power_slider):
            self.fan_power_slider.setValue(
                fan_settings["fan_pwm"] or 100  # 0 = PWM off = full strength
            )
        with QSignalBlocker(self.fan_auto_box):
            self.fan_auto_box.setChecked(fan_settings["auto_mode"])

    def set_fan_pwm_warning(self):
        if self.fan_power_slider.value() != 100:
            pixmapi = getattr(QtWidgets.QStyle.StandardPixmap, "SP_MessageBoxWarning")
            icon = self._style.standardIcon(pixmapi)
            self.fan_pwm_warning.setPixmap(icon.pixmap(16, 16))
            self.fan_pwm_warning.setToolTip(
                "Throttling the fan (not recommended on this hardware rev)"
            )
        else:
            self.fan_pwm_warning.setPixmap(QtGui.QPixmap())
            self.fan_pwm_warning.setToolTip("")

    @pyqtSlot(ThermostatConnectionState)
    def thermostat_state_change_handler(self, state):
        if state == ThermostatConnectionState.DISCONNECTED:
            self.fan_pwm_warning.setPixmap(QtGui.QPixmap())
            self.fan_pwm_warning.setToolTip("")

    @pyqtSlot("QVariantMap")
    def hw_rev(self, hw_rev):
        self.hw_rev_data = hw_rev
        self.fan_group.setEnabled(self.hw_rev_data["settings"]["fan_available"])

    @asyncSlot(int)
    async def fan_set_request(self, value):
        assert self._thermostat.connected()

        if self.fan_auto_box.isChecked():
            with QSignalBlocker(self.fan_auto_box):
                self.fan_auto_box.setChecked(False)
        await self._thermostat.set_fan(value)
        if not self.hw_rev_data["settings"]["fan_pwm_recommended"]:
            self.set_fan_pwm_warning()

    @asyncSlot(int)
    async def fan_auto_set_request(self, enabled):
        assert self._thermostat.connected()

        if enabled:
            await self._thermostat.set_fan("auto")
            self.fan_update(await self._thermostat.get_fan())
        else:
            await self.thermostat.set_fan(self.fan_power_slider.value())

    @asyncSlot(bool)
    async def reset_request(self, _):
        assert self._thermostat.connected()

        await self._thermostat.reset()
        await self._thermostat.end_session()
        self._thermostat.connection_state = ThermostatConnectionState.DISCONNECTED

    @asyncSlot(bool)
    async def dfu_request(self, _):
        assert self._thermostat.connected()

        await self._thermostat.dfu()
        await self._thermostat.end_session()
        self._thermostat.connection_state = ThermostatConnectionState.DISCONNECTED

    @asyncSlot(bool)
    async def net_settings_request(self, _):
        assert self._thermostat.connected()

        ipv4 = await self._thermostat.get_ipv4()
        net_settings_input_diag = NetSettingsInputDiag(ipv4["addr"])
        net_settings_input_diag.set_ipv4_act.connect(self.set_net_settings_request)

    @asyncSlot(str)
    async def set_net_settings_request(self, ipv4_settings):
        assert self._thermostat.connected()

        await self._thermostat.set_ipv4(ipv4_settings)
        await self._thermostat.end_session()
        self._thermostat.connection_state = ThermostatConnectionState.DISCONNECTED

    def _setup_menu_items(self):
        self.addAction(self._setup_fan_group())

        self.reset_action = QtGui.QAction("Reset Thermostat", self)
        self.reset_action.triggered.connect(self.reset_request)
        self.addAction(self.reset_action)

        self.dfu_action = QtGui.QAction("Enter DFU Mode", self)
        self.dfu_action.triggered.connect(self.dfu_request)
        self.addAction(self.dfu_action)

        self.ipv4_action = QtGui.QAction("Set IPv4 Settings", self)
        self.ipv4_action.triggered.connect(self.net_settings_request)
        self.addAction(self.ipv4_action)

        @asyncSlot(bool)
        async def load(_):
            await self._thermostat.load_cfg()

            self._info_box.display_info_box(
                "Config loaded", "All channel configs have been loaded from flash."
            )

        self.load_config_action = QtGui.QAction("Load Config", self)
        self.load_config_action.triggered.connect(load)
        self.addAction(self.load_config_action)

        @asyncSlot(bool)
        async def save(_):
            await self._thermostat.save_cfg()

            self._info_box.display_info_box(
                "Config saved", "All channel configs have been saved to flash."
            )

        self.save_config_action = QtGui.QAction("Save Config", self)
        self.save_config_action.triggered.connect(save)
        self.addAction(self.save_config_action)

        def about_thermostat():
            QtWidgets.QMessageBox.about(
                self,
                "About Thermostat",
                f"""
                <h1>Sinara 8451 Thermostat v{self.hw_rev_data['rev']['major']}.{self.hw_rev_data['rev']['minor']}</h1>

                <br>

                <h2>Settings:</h2>
                Default fan curve:
                    a = {self.hw_rev_data['settings']['fan_k_a']},
                    b = {self.hw_rev_data['settings']['fan_k_b']},
                    c = {self.hw_rev_data['settings']['fan_k_c']}
                <br>
                Fan PWM range:
                    {self.hw_rev_data['settings']['min_fan_pwm']} \u2013 {self.hw_rev_data['settings']['max_fan_pwm']}
                <br>
                Fan PWM frequency: {self.hw_rev_data['settings']['fan_pwm_freq_hz']} Hz
                <br>
                Fan available: {self.hw_rev_data['settings']['fan_available']}
                <br>
                Fan PWM recommended: {self.hw_rev_data['settings']['fan_pwm_recommended']}
                """,
            )

        self.about_action = QtGui.QAction("About Thermostat", self)
        self.about_action.triggered.connect(about_thermostat)
        self.addAction(self.about_action)

    def _setup_fan_group(self):
        # Fan settings
        self.fan_group = QtWidgets.QWidget()
        self.fan_group.setEnabled(False)
        self.fan_group.setMinimumWidth(40)
        fan_layout = QtWidgets.QHBoxLayout(self.fan_group)
        fan_layout.setSpacing(9)

        fan_label = QtWidgets.QLabel(parent=self.fan_group)
        fan_label.setMinimumWidth(40)
        fan_label.setMaximumWidth(40)
        fan_label.setBaseSize(QtCore.QSize(40, 0))

        fan_layout.addWidget(fan_label)
        self.fan_power_slider = QtWidgets.QSlider(
            QtCore.Qt.Orientation.Horizontal, parent=self.fan_group
        )
        self.fan_power_slider.setMinimumWidth(200)
        self.fan_power_slider.setMaximumWidth(200)
        self.fan_power_slider.setBaseSize(QtCore.QSize(200, 0))
        self.fan_power_slider.setRange(1, 100)
        fan_layout.addWidget(self.fan_power_slider)

        self.fan_auto_box = QtWidgets.QCheckBox(parent=self.fan_group)
        self.fan_auto_box.setMinimumWidth(70)
        self.fan_auto_box.setMaximumWidth(70)
        fan_layout.addWidget(self.fan_auto_box)
        self.fan_pwm_warning = QtWidgets.QLabel(parent=self.fan_group)
        self.fan_pwm_warning.setMinimumSize(QtCore.QSize(16, 0))
        fan_layout.addWidget(self.fan_pwm_warning)

        self.fan_power_slider.valueChanged.connect(self.fan_set_request)
        self.fan_auto_box.stateChanged.connect(self.fan_auto_set_request)
        self._thermostat.fan_update.connect(self.fan_update)

        fan_label.setToolTip("Adjust the fan")
        fan_label.setText("Fan:")
        self.fan_auto_box.setText("Auto")

        fan = QtWidgets.QWidgetAction(self)
        fan.setDefaultWidget(self.fan_group)
        return fan
