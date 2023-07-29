from utils import cmd, bail, print_header
from dataclasses import dataclass
import json


@dataclass
class ImageSpec:
    name: str
    dockerfile: str
    command: str

    @classmethod
    def from_dict(cls, data: dict):
        return cls(data.get("name"), data.get("dockerfile"), data.get("command"))


def build():
    with open("deploy/base/_meta.json", "r") as f:
        metadatas: list[ImageSpec] = json.load(f)
        metadatas = [ImageSpec.from_dict(metadata) for metadata in metadatas]

    for metadata in metadatas:
        build_metadata(metadata)


def build_single(name: str):
    with open("deploy/base/_meta.json", "r") as f:
        metadatas: list[ImageSpec] = json.load(f)
        metadatas = [ImageSpec.from_dict(metadata) for metadata in metadatas]

    for metadata in metadatas:
        if metadata.name == name:
            build_metadata(metadata)
            return


def build_metadata(metadata: ImageSpec):
    print(f"> Building image `{metadata.name}`")
    if (
        cmd(
            f"docker build --file ./deploy/base/{metadata.dockerfile} -t hydra-{metadata.name} ./deploy/base"
        )
        != 0
    ):
        bail(f"docker build for `{metadata.name}` exited with a non-zero exit code.")

    print("> Saving image to tarball")
    if (
        cmd(f"docker save hydra-{metadata.name} > images/hydra-{metadata.name}.tar")
        != 0
    ):
        bail(f"docker save for `{metadata.name}` exited with a non-zero exit code.")
