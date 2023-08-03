import os
from termcolor import colored
from hashlib import md5
import functools


def hash_directory(path: str) -> str:
    hash = md5()

    for root, dirs, files in os.walk(path):
        for file in files:
            with open(os.path.join(root, file), "rb") as f:
                hash.update(f.read())

    return hash.hexdigest()


def hash_file(path: str) -> str:
    hash = md5()

    with open(path, "rb") as f:
        hash.update(f.read())

    return hash.hexdigest()


def compute_cache_key(path: str) -> str:
    if os.path.isdir(path):
        return hash_directory(path)
    else:
        return hash_file(path)


def is_fs_cached(key: str, paths: list[str]) -> bool:
    """
    Usage:

    ```
    if fs_cached("key", "path"):
        # This operation is cached, do not do it again
        return

    # Do stuff
    ```
    """
    if not os.path.exists(".cache"):
        os.mkdir(".cache")

    does_cache_exist = os.path.exists(f".cache/{key}")
    if not does_cache_exist:
        return False

    cache_keys = [compute_cache_key(path) for path in paths]
    with open(f".cache/{key}", "r") as f:
        return f.read() == "\n".join(cache_keys)


def add_to_cache(key: str, paths: list[str]):
    cache_keys = [compute_cache_key(path) for path in paths]
    with open(f".cache/{key}", "w") as f:
        f.write("\n".join(cache_keys))


def fs_cache(cache_key: str = "", paths: list[str] = []):
    def decorator(func):
        def wrapper(*args, **kwargs):
            if is_fs_cached(cache_key, paths):
                print(
                    f"{colored('>', 'magenta')} cache {colored('hit', 'magenta')} (`{cache_key}`)"
                )
                return

                print(
                    f"{colored('x', 'red')} cache {colored('miss', 'magenta')} (`image-{metadata.name}`)"
                )

            func(*args, **kwargs)

            add_to_cache(cache_key, paths)

        return wrapper

    return decorator
