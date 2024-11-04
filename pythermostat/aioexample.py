import asyncio
from pythermostat.aioclient import AsyncioClient


async def poll_for_reports(thermostat_aio):
    while True:
        print(await thermostat_aio.get_report())
        await asyncio.sleep(0.05)


async def poll_for_settings(thermostat_aio):
    while True:
        await asyncio.sleep(1)
        print(await thermostat_aio.get_output())
        print(await thermostat_aio.get_pid())
        print(await thermostat_aio.get_fan())
        print(await thermostat_aio.get_postfilter())
        print(await thermostat_aio.get_b_parameter())


async def main():
    thermostat_aio = AsyncioClient()
    await thermostat_aio.connect()
    await thermostat_aio.set_param("b-p", 1, "t0", 20)
    print(await thermostat_aio.get_output())
    print(await thermostat_aio.get_pid())
    print(await thermostat_aio.get_fan())
    print(await thermostat_aio.get_postfilter())
    print(await thermostat_aio.get_b_parameter())

    # Poll both reports and settings, at different rates
    async with asyncio.TaskGroup() as tg:
        tg.create_task(poll_for_reports(thermostat_aio))
        tg.create_task(poll_for_settings(thermostat_aio))


asyncio.run(main())
