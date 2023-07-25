from termcolor import colored
import os
import subprocess
import sys


def print_header(text):
    COLORED_HYDRA = colored("Hydra", "magenta")
    print(f">>> {COLORED_HYDRA} - {text}")


def bail(text):
    COLORED_BAILED = colored("Bailed", "red")
    print(f"\n{COLORED_BAILED} - {text}", file=sys.stderr)
    exit(1)


def cmd(command: str) -> int:
    """
    Run a command and print it to the console.
    Returns the exit code of the process.
    """
    COLORED_DOLLAR_SIGN = colored("$", "magenta")
    print(f"{COLORED_DOLLAR_SIGN} {command}")
    return os.system(command)


def get_root_dir():
    return (
        subprocess.Popen(
            ["git", "rev-parse", "--show-toplevel"], stdout=subprocess.PIPE
        )
        .communicate()[0]
        .rstrip()
        .decode("utf-8")
    )
