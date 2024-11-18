import time
from pythermostat.client import Client

tec = Client() #(host="localhost", port=6667)
tec.set_param("b-p", 1, "t0", 20)
print(tec.get_output())
print(tec.get_pid())
print(tec.get_output())
print(tec.get_postfilter())
print(tec.get_b_parameter())
while True:
    print(tec.get_report())
    time.sleep(0.05)
