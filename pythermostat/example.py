import time
from pythermostat.client import Client

thermostat = Client() #(host="localhost", port=6667)
thermostat.set_param("b-p", 1, "t0", 20)
print(thermostat.get_output())
print(thermostat.get_pid())
print(thermostat.get_output())
print(thermostat.get_postfilter())
print(thermostat.get_b_parameter())
while True:
    print(thermostat.get_report())
    time.sleep(0.05)
