from PyQt6.QtCore import QObject, pyqtSlot, pyqtSignal
from qasync import asyncSlot
from pythermostat.autotune import PIDAutotuneState, PIDAutotune


class PIDAutoTuner(QObject):
    autotune_state_changed = pyqtSignal(int, PIDAutotuneState)

    def __init__(self, parent, thermostat, num_of_channel):
        super().__init__(parent)

        self._thermostat = thermostat
        self._thermostat.report_update.connect(self.tick)

        self.autotuners = [PIDAutotune(25) for _ in range(num_of_channel)]
        self.target_temp = [20.0 for _ in range(num_of_channel)]
        self.test_current = [1.0 for _ in range(num_of_channel)]
        self.temp_swing = [1.5 for _ in range(num_of_channel)]
        self.lookback = [3.0 for _ in range(num_of_channel)]
        self.sampling_interval = [1 / 16.67 for _ in range(num_of_channel)]

    def set_params(self, params_name, ch, val):
        getattr(self, params_name)[ch] = val

    def get_state(self, ch):
        return self.autotuners[ch].state()

    def load_params_and_set_ready(self, ch):
        self.autotuners[ch].set_param(
            self.target_temp[ch],
            self.test_current[ch] / 1000,
            self.temp_swing[ch],
            1 / self.sampling_interval[ch],
            self.lookback[ch],
        )
        self.autotuners[ch].set_ready()
        self.autotune_state_changed.emit(ch, self.autotuners[ch].state())

    async def stop_pid_from_running(self, ch):
        self.autotuners[ch].set_off()
        self.autotune_state_changed.emit(ch, self.autotuners[ch].state())
        if self._thermostat.connected():
            await self._thermostat.set_param("output", ch, "i_set", 0)

    @asyncSlot(list)
    async def tick(self, report):
        for channel_report in report:
            ch = channel_report["channel"]

            self.sampling_interval[ch] = channel_report["interval"]

            # TODO: Skip when PID Autotune or emit error message if NTC is not connected
            if channel_report["temperature"] is None:
                continue

            match self.autotuners[ch].state():
                case (
                    PIDAutotuneState.READY
                    | PIDAutotuneState.RELAY_STEP_UP
                    | PIDAutotuneState.RELAY_STEP_DOWN
                ):
                    self.autotuners[ch].run(
                        channel_report["temperature"], channel_report["time"]
                    )
                    await self._thermostat.set_param(
                        "output", ch, "i_set", self.autotuners[ch].output()
                    )
                case PIDAutotuneState.SUCCEEDED:
                    kp, ki, kd = self.autotuners[ch].get_pid_parameters("tyreus-luyben")
                    self.autotuners[ch].set_off()
                    self.autotune_state_changed.emit(ch, self.autotuners[ch].state())

                    await self._thermostat.set_param("pid", ch, "kp", kp)
                    await self._thermostat.set_param("pid", ch, "ki", ki)
                    await self._thermostat.set_param("pid", ch, "kd", kd)
                    await self._thermostat.set_param("output", ch, "pid")

                    await self._thermostat.set_param(
                        "pid", ch, "target", self.target_temp[ch]
                    )
                case PIDAutotuneState.FAILED:
                    self.autotuners[ch].set_off()
                    self.autotune_state_changed.emit(ch, self.autotuners[ch].state())
                    await self._thermostat.set_param("output", ch, "i_set", 0)
