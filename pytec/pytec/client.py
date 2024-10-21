import socket
import json
import logging
import time

class CommandError(Exception):
    pass

class Client:
    def __init__(self, host="192.168.1.26", port=23, timeout=None):
        self._socket = socket.create_connection((host, port), timeout)
        self._lines = [""]
        self._check_zero_limits()

    def _check_zero_limits(self):
        output_report = self.get_output()
        for output_channel in output_report:
            for limit in ["max_i_neg", "max_i_pos", "max_v"]:
                if output_channel[limit] == 0.0:
                    logging.warning("`{}` limit is set to zero on channel {}".format(limit, output_channel["channel"]))

    def _read_line(self):
        # read more lines
        while len(self._lines) <= 1:
            chunk = self._socket.recv(4096)
            if not chunk:
                return None
            buf = self._lines[-1] + chunk.decode('utf-8', errors='ignore')
            self._lines = buf.split("\n")

        line = self._lines[0]
        self._lines = self._lines[1:]
        return line

    def _command(self, *command):
        self._socket.sendall((" ".join(command) + "\n").encode('utf-8'))

        line = self._read_line()
        response = json.loads(line)
        if "error" in response:
            raise CommandError(response["error"])
        return response

    def _get_conf(self, topic):
        result = [None, None]
        for item in self._command(topic):
            result[int(item["channel"])] = item
        return result

    def get_output(self):
        """Retrieve output limits for the TEC

        Example::
            [{'channel': 0,
              'center': 'vref',
              'i_set': -0.02002179650216762,
              'max_i_neg': 2.0,
              'max_v': 3.988,
              'max_i_pos': 2.0,
              'polarity': 'normal',
             {'channel': 1,
              'center': 'vref',
              'i_set': -0.02002179650216762,
              'max_i_neg': 2.0,
              'max_v': 3.988,
              'max_i_pos': 2.0}
              'polarity': 'normal',
            ]
        """
        return self._get_conf("output")

    def get_pid(self):
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
        return self._get_conf("pid")

    def get_steinhart_hart(self):
        """Retrieve Steinhart-Hart parameters for resistance to temperature conversion

        Example::
            [{'params': {'b': 3800.0, 'r0': 10000.0, 't0': 298.15}, 'channel': 0},
             {'params': {'b': 3800.0, 'r0': 10000.0, 't0': 298.15}, 'channel': 1}]
        """
        return self._get_conf("s-h")

    def get_postfilter(self):
        """Retrieve DAC postfilter configuration

        Example::
            [{'rate': None, 'channel': 0},
             {'rate': 21.25, 'channel': 1}]
        """
        return self._get_conf("postfilter")

    def report_mode(self):
        """Start reporting measurement values

        Example of yielded data::
            {'channel': 0,
             'time': 2302524,
             'adc': 0.6199188965423515,
             'sens': 6138.519310282602,
             'temperature': 36.87032392655527,
             'pid_engaged': True,
             'i_set': 2.0635816680889123,
             'vref': 1.494,
             'dac_value': 2.527790834044456,
             'dac_feedback': 2.523,
             'i_tec': 2.331,
             'tec_i': 2.0925,
             'tec_u_meas': 2.5340000000000003,
             'pid_output': 2.067581958092247}
        """
        while True:
            self._socket.sendall("report\n".encode('utf-8'))
            line = self._read_line()
            if not line:
                break
            try:
                yield json.loads(line)
            except json.decoder.JSONDecodeError:
                pass
            time.sleep(0.05)

    def set_param(self, topic, channel, field="", value=""):
        """Set configuration parameters

        Examples::
            tec.set_param("output", 0, "max_v", 2.0)
            tec.set_param("pid", 1, "output_max", 2.5)
            tec.set_param("s-h", 0, "t0", 20.0)
            tec.set_param("center", 0, "vref")
            tec.set_param("postfilter", 1, 21)

        See the firmware's README.md for a full list.
        """
        if type(value) is float:
            value = "{:f}".format(value)
        if type(value) is not str:
            value = str(value)
        self._command(topic, str(channel), field, value)

    def power_up(self, channel, target):
        """Start closed-loop mode"""
        self.set_param("pid", channel, "target", value=target)
        self.set_param("output", channel, "pid")

    def save_config(self):
        """Save current configuration to EEPROM"""
        self._command("save")

    def load_config(self):
        """Load current configuration from EEPROM"""
        self._command("load")
