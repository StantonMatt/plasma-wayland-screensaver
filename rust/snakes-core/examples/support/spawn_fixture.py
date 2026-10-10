# SPDX-License-Identifier: GPL-3.0-or-later
"""Checked source transformations for isolated singleton spawn experiments."""
import re


def enabled_kinds(source, kind):
    """Match the declaration, independent of which production kinds are enabled."""
    source, count = re.subn(
        r"pub const ENABLED_KINDS:\s*&\[EffectKind\]\s*=\s*&\[[^;]*\];",
        f"pub const ENABLED_KINDS: &[EffectKind] = &[EffectKind::{kind}];", source)
    if count != 1:
        raise ValueError(f"Expected one ENABLED_KINDS declaration, found {count}")
    return source


def singleton_items(source):
    """Change the marked repeat policy shared by both spawn predicates.

    Other exclusions (including an active Whirlpool) remain production rules.
    The marker and boolean declaration in items.rs are the tooling contract;
    predicate formatting and additional exclusions do not affect the fixture.
    """
    marker = "// spawn-fixture: repeat-exclusion"
    count = source.count(marker)
    if count != 1:
        raise ValueError(f"Expected one {marker} marker in items.rs, found {count}")
    source, count = re.subn(
        r"(?m)^([ \t]*// spawn-fixture: repeat-exclusion[ \t]*\n"
        r"[ \t]*let\s+exclude_repeat\s*=\s*)true(\s*;)",
        r"\g<1>effects::ENABLED_KINDS.len() > 1\g<2>", source)
    if count != 1:
        raise ValueError("Spawn fixture contract in items.rs requires the marker "
                         "followed by `let exclude_repeat = true;` (once, unmodified)")
    return source
