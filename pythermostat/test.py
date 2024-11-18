import argparse
from contextlib import contextmanager
from pytec.client import Client


CHANNELS = 2


def get_argparser():
    parser = argparse.ArgumentParser(description="Thermostat hardware testing script")

    parser.add_argument("host", metavar="HOST", default="192.168.1.26", nargs="?")
    parser.add_argument("port", metavar="PORT", default=23, nargs="?")
    parser.add_argument(
        "-r",
        "--testing_resistance",
        default=10_000,
        help="Testing resistance value through SENS pin in Ohms",
    )
    parser.add_argument(
        "-d",
        "--deviation",
        default=1,
        help="Allowed deviation of resistance in percentage",
    )

    return parser


def main():
    args = get_argparser().parse_args()

    min_allowed_resistance = args.testing_resistance * (1 - args.deviation / 100)
    max_allowed_resistance = args.testing_resistance * (1 + args.deviation / 100)

    print(min_allowed_resistance, max_allowed_resistance)

    thermostat = Client(args.host, args.port)
    for channel in range(CHANNELS):
        print(f"Channel {channel} is active")

        print("Checking resistance through SENS input ....", end=" ")
        sens_resistance = thermostat.get_report()[channel]["sens"]
        if sens_resistance is not None:
            print(sens_resistance, "Ω")
            if min_allowed_resistance <= sens_resistance <= max_allowed_resistance:
                print("PASSED")
            else:
                print("FAILED")
        else:
            print("Floating SENS input! Is the channel connected?")

        with preserve_thermostat_output_settings(thermostat, channel):
            test_output_settings = {
                "max_i_pos": 2,
                "max_i_neg": 2,
                "max_v": 4,
                "i_set": 0.1,
                "polarity": "normal",
            }
            for field, value in test_output_settings.items():
                thermostat.set_param("output", channel, field, value)

            input(f"Check if channel {channel} current = 0.1 A, and press ENTER...")

        input(f"Channel {channel} testing done, press ENTER to continue.")
        print()

    print("Testing complete.")


@contextmanager
def preserve_thermostat_output_settings(client, channel):
    original_output_settings = client.get_output()[channel]
    yield original_output_settings
    for setting in "max_i_pos", "max_i_neg", "max_v", "i_set", "polarity":
        client.set_param("output", channel, setting, original_output_settings[setting])


if __name__ == "__main__":
    main()
