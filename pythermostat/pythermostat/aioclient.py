import asyncio
import json
import logging


class CommandError(Exception):
    pass


class AsyncioClient:
    def __init__(self):
        self._reader = None
        self._writer = None
        self._read_lock = asyncio.Lock()

    async def connect(self, host="192.168.1.26", port=23):
        """Connect to Thermostat at specified host and port.

        Example::
            thermostat = AsyncioClient()
            await client.connect()
        """
        self._reader, self._writer = await asyncio.open_connection(host, port)
        await self._check_zero_limits()

    def connected(self):
        """Returns True if client is connected"""
        return self._writer is not None

    async def disconnect(self):
        """Disconnect from the Thermostat"""

        if self._writer is None:
            return

        # Reader needn't be closed
        self._writer.close()
        await self._writer.wait_closed()
        self._reader = None
        self._writer = None

    async def _check_zero_limits(self):
        output_report = await self.get_output()
        for output_channel in output_report:
            for limit in ["max_i_neg", "max_i_pos", "max_v"]:
                if output_channel[limit] == 0.0:
                    logging.warning(
                        "`%s` limit is set to zero on channel %d",
                        limit,
                        output_channel["channel"],
                    )

    async def _read_line(self):
        # read 1 line
        async with self._read_lock:
            chunk = await self._reader.readline()
        return chunk.decode("utf-8", errors="ignore")

    async def _read_write(self, command):
        self._writer.write(((" ".join(command)).strip() + "\n").encode("utf-8"))
        await self._writer.drain()

        return await self._read_line()

    async def _command(self, *command):
        line = await self._read_write(command)

        response = json.loads(line)
        if "error" in response:
            raise CommandError(response["error"])
        return response

    async def _get_conf(self, topic):
        result = [None, None]
        for item in await self._command(topic):
            result[int(item["channel"])] = item
        return result

    async def get_output(self):
        """Retrieve output limits for the TEC

        Example::
            [{'channel': 0,
              'center': 'vref',
              'i_set': -0.02002179650216762,
              'max_i_neg': 2.0,
              'max_v': : 3.988,
              'max_i_pos': 2.0,
              'polarity': 'normal'},
             {'channel': 1,
              'center': 'vref',
              'i_set': -0.02002179650216762,
              'max_i_neg': 2.0,
              'max_v': : 3.988,
              'max_i_pos': 2.0,
              'polarity': 'normal'},
            ]
        """
        return await self._get_conf("output")

    async def get_pid(self):
        """Retrieve PID control state

        Example::
            [{'channel': 0,
              'parameters': {
                  'kp': 10.0,
                  'ki': 0.02,
                  'kd': 0.0,
                  'output_min': 0.0,
                  'output_max': 3.0},
              'target': 37.0},
             {'channel': 1,
              'parameters': {
                  'kp': 10.0,
                  'ki': 0.02,
                  'kd': 0.0,
                  'output_min': 0.0,
                  'output_max': 3.0},
              'target': 36.5}]
        """
        return await self._get_conf("pid")

    async def get_b_parameter(self):
        """
        Retrieve B-Parameter equation parameters for resistance to
        temperature conversion

        Example::
            [{'params': {'b': 3800.0, 'r0': 10000.0, 't0': 298.15}, 'channel': 0},
             {'params': {'b': 3800.0, 'r0': 10000.0, 't0': 298.15}, 'channel': 1}]
        """
        return await self._get_conf("b-p")

    async def get_postfilter(self):
        """Retrieve DAC postfilter configuration

        Example::
            [{'rate': None, 'channel': 0},
             {'rate': 21.25, 'channel': 1}]
        """
        return await self._get_conf("postfilter")

    async def get_report(self):
        """Obtain one-time report on measurement values

        Example of yielded data:
            {'channel': 0,
             'time': 2302524,
             'interval': 0.12
             'adc': 0.6199188965423515,
             'sens': 6138.519310282602,
             'temperature': 36.87032392655527,
             'pid_engaged': True,
             'i_set': 2.0635816680889123,
             'dac_value': 2.527790834044456,
             'dac_feedback': 2.523,
             'i_tec': 2.331,
             'tec_i': 2.0925,
             'tec_u_meas': 2.5340000000000003,
             'pid_output': 2.067581958092247}
        """
        return await self._command("report")

    async def get_ipv4(self):
        """Get the IPv4 settings of the Thermostat"""
        return await self._command("ipv4")

    async def get_fan(self):
        """Get Thermostat current fan settings"""
        return await self._command("fan")

    async def get_hwrev(self):
        """Get Thermostat hardware revision"""
        return await self._command("hwrev")

    async def set_param(self, topic, channel, field="", value=""):
        """Set configuration parameters

        Examples::
            await thermostat.set_param("output", 0, "max_v", 2.0)
            await thermostat.set_param("pid", 1, "output_max", 2.5)
            await thermostat.set_param("b-p", 0, "t0", 20.0)
            await thermostat.set_param("center", 0, "vref")
            await thermostat.set_param("postfilter", 1, 21)

        See the firmware's README.md for a full list.
        """
        if isinstance(value, float):
            value = f"{value:f}"
        if not isinstance(value, str):
            value = str(value)
        await self._command(topic, str(channel), field, value)

    async def power_up(self, channel, target):
        """Start closed-loop mode"""
        await self.set_param("pid", channel, "target", value=target)
        await self.set_param("output", channel, "pid")

    async def save_config(self, channel=""):
        """Save current configuration to EEPROM"""
        await self._command("save", str(channel))
        if channel == "":
            await self._read_line()  # Read the extra {}

    async def load_config(self, channel=""):
        """Load current configuration from EEPROM"""
        await self._command("load", str(channel))
        if channel == "":
            await self._read_line()  # Read the extra {}

    async def reset(self):
        """Reset the Thermostat

        The client is disconnected as the TCP session is terminated.
        """
        self._writer.write("reset\n".encode("utf-8"))
        await self._writer.drain()

        await self.disconnect()

    async def enter_dfu_mode(self):
        """Put the Thermostat in DFU mode

        The client is disconnected as the Thermostat stops responding to
        TCP commands in DFU mode. To exit it, submit a DFU leave request
        or power-cycle the Thermostat.
        """
        self._writer.write("dfu\n".encode("utf-8"))
        await self._writer.drain()

        await self.disconnect()

    async def set_fan(self, power="auto"):
        """Set fan power with values from 1 to 100. If omitted, set according to fcurve"""
        await self._command("fan", str(power))

    async def set_fcurve(self, a=1.0, b=0.0, c=0.0):
        """Set fan controller curve coefficients"""
        await self._command("fcurve", str(a), str(b), str(c))
