from typing import Self


class Std:

    log: list[str] = []

    def __init__(self: Self) -> None:
        self.log = []

    def print(self: Self, message: str) -> None:
        self.log.append(message)

    def clear(self: Self) -> None:
        self.log = []
