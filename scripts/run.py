#!/bin/python3

import build.subcommand as build_subcommand
import dev as dev_subcommand
from argparse import ArgumentParser

parser = ArgumentParser(
    prog="Hydra Script Runner",
)
subparsers = parser.add_subparsers(dest="sub")
dev_subcommand.setup_parser(subparsers.add_parser("dev"))
build_subcommand.setup_build_parser(subparsers.add_parser("build"))
build_subcommand.setup_image_parser(subparsers.add_parser("image"))

args = parser.parse_args()

if args.sub == "build":
    build_subcommand.run_build(args)
elif args.sub == "dev":
    dev_subcommand.run(args)
elif args.sub == "image":
    build_subcommand.run_build_image(args)
