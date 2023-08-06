from argparse import ArgumentParser, Namespace
from utils import print_header, cmd, get_root_dir

from build.server import build as build_server
from build.daemon import build as build_daemon
from build.images import build as build_images, build_single as build_single_image
from build.proxy import build as build_proxy


def setup_build_parser(parser: ArgumentParser):
    parser.add_argument(
        "component",
        choices=["all", "images", "daemon", "server", "proxy"],
        default="all",
    )


def setup_image_parser(parser: ArgumentParser):
    subparser = parser.add_subparsers()
    parser = subparser.add_parser("build")
    parser.add_argument("image")


def run_build(args: Namespace):
    print_header(f"Building component `{args.component}`")

    cmd(f"cd {get_root_dir()}")

    if args.component == "server":
        build_daemon()
        build_images()
        build_server()
    elif args.component == "proxy":
        build_proxy()
    elif args.component == "daemon":
        build_daemon()
    elif args.component == "images":
        build_daemon()
        build_images()
    elif args.component == "all":
        build_daemon()
        build_images()
        build_server()
        build_proxy()


def run_build_image(args: Namespace):
    print_header(f"Building image `{args.image}`")
    build_daemon()
    build_single_image(args.image)
