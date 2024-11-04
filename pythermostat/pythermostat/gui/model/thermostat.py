import asyncio
import logging
from enum import Enum
from PyQt6.QtCore import pyqtSignal, QObject, pyqtSlot
from qasync import asyncSlot
from pythermostat.aioclient import AsyncioClient
from pythermostat.gui.model.property import Property, PropertyMeta


class ThermostatConnectionState(Enum):
    DISCONNECTED = "disconnected"
    CONNECTING = "connecting"
    CONNECTED = "connected"


class Thermostat(QObject, metaclass=PropertyMeta):
    connection_state = Property(ThermostatConnectionState)
    hw_rev = Property(dict)
    fan = Property(dict)
    thermistor = Property(list)
    pid = Property(list)
    output = Property(list)
    postfilter = Property(list)
    report = Property(list)

    connection_error = pyqtSignal()

    NUM_CHANNELS = 2

    def __init__(self, parent, update_s, disconnect_cb=None):
        super().__init__(parent)

        self._update_s = update_s
        self._client = AsyncioClient()
        self._watch_task = None
        self._update_params_task = None
        self.disconnect_cb = disconnect_cb
        self.connection_state = ThermostatConnectionState.DISCONNECTED

    async def start_session(self, host, port):
        await self._client.connect(host, port)
        self.hw_rev = await self._client.get_hwrev()

    @asyncSlot()
    async def end_session(self):
        self.stop_watching()

        if self.disconnect_cb is not None:
            if asyncio.iscoroutinefunction(self.disconnect_cb):
                await self.disconnect_cb()
            else:
                self.disconnect_cb()

        await self._client.disconnect()

    def start_watching(self):
        self._watch_task = asyncio.create_task(self.run())

    def stop_watching(self):
        if self._watch_task is not None:
            self._watch_task.cancel()
            self._watch_task = None
            self._update_params_task.cancel()
            self._update_params_task = None

    async def run(self):
        self._update_params_task = asyncio.create_task(self.update_params())
        while True:
            if self._update_params_task.done():
                try:
                    self._update_params_task.result()
                except OSError:
                    logging.error(
                        "Encountered an error while polling for information from Thermostat.",
                        exc_info=True,
                    )
                    await self.end_session()
                    self.connection_state = ThermostatConnectionState.DISCONNECTED
                    self.connection_error.emit()
                    return
                self._update_params_task = asyncio.create_task(self.update_params())
            await asyncio.sleep(self._update_s)

    async def update_params(self):
        (
            self.fan,
            self.output,
            self.report,
            self.pid,
            self.thermistor,
            self.postfilter,
        ) = await asyncio.gather(
            self._client.get_fan(),
            self._client.get_output(),
            self._client.get_report(),
            self._client.get_pid(),
            self._client.get_b_parameter(),
            self._client.get_postfilter(),
        )

    def connected(self):
        return self._client.connected()

    @pyqtSlot(float)
    def set_update_s(self, update_s):
        self._update_s = update_s

    async def set_ipv4(self, ipv4):
        await self._client.set_param("ipv4", ipv4)

    async def get_ipv4(self):
        return await self._client.get_ipv4()

    @asyncSlot()
    async def save_cfg(self, ch=""):
        await self._client.save_config(ch)

    @asyncSlot()
    async def load_cfg(self, ch=""):
        await self._client.load_config(ch)

    async def dfu(self):
        await self._client.enter_dfu_mode()

    async def reset(self):
        await self._client.reset()

    async def set_fan(self, power="auto"):
        await self._client.set_fan(power)

    async def get_fan(self):
        return await self._client.get_fan()

    async def set_param(self, topic, channel, field="", value=""):
        await self._client.set_param(topic, channel, field, value)
