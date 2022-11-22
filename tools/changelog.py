#!/usr/bin/env python3
# Copyright 2022 Science project contributors.
# Licensed under the Apache License, Version 2.0 (see LICENSE).

from __future__ import annotations

import sys
from argparse import ArgumentParser
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Mapping, TextIO

import marko
from marko.block import Heading
from marko.element import Element
from marko.inline import RawText
from marko.md_renderer import MarkdownRenderer


@dataclass(frozen=True)
class Release:
    elements: Iterable[Element]


def extract_level_heading(element: Element, level: int) -> str | None:
    if (
        isinstance(element, Heading)
        and element.level == level
        and len(element.children) == 1
        and isinstance(element.children[0], RawText)
    ):
        return element.children[0].children
    return None


@dataclass(frozen=True)
class Changelog:
    @classmethod
    def parse(cls, changelog: str) -> Changelog:
        document = marko.parse(changelog)
        releases = {}
        current_release = None
        for child in document.children:
            heading = extract_level_heading(child, level=2)
            if heading is not None:
                current_release = [child]
                releases[heading] = current_release
            elif current_release is not None:
                current_release.append(child)
        return cls({release: Release(elements) for release, elements in releases.items()})

    releases: Mapping[str, Release]


def render_version(changelog_path: Path, version: str, output: TextIO) -> str | None:
    changelog = Changelog.parse(changelog_path.read_text())
    release = changelog.releases.get(version)
    if release is None:
        return f"No change log entry for release {version} was found in {changelog_path}."

    with MarkdownRenderer() as renderer:
        for element in release.elements:
            output.write(renderer.render(element))

    return None


if __name__ == "__main__":
    parser = ArgumentParser()
    parser.add_argument("version", help="The version to extract change log entries for.")
    options = parser.parse_args()
    result = render_version(
        changelog_path=Path("CHANGES.md"), version=options.version, output=sys.stdout
    )
    sys.exit(result)
