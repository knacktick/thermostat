from setuptools import setup, find_packages

setup(
    name="pythermostat",
    version="0.0",
    author="M-Labs",
    url="https://git.m-labs.hk/M-Labs/thermostat",
    description="Control TEC",
    license="GPLv3",
    install_requires=["setuptools"],
    packages=find_packages(),
    entry_points={
        "gui_scripts": [
            "thermostat_plot = pythermostat.plot:main",
        ],
        "console_scripts": [
            "thermostat_autotune = pythermostat.autotune:main",
            "thermostat_test = pythermostat.test:main",
        ]
    },
)
