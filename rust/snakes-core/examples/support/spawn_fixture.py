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
    """Keep repeat exclusion in production, permit repeats in singleton fixtures."""
    for old, new in [
        ("filter(|&&k|k!=self.last_item_kind)",
         "filter(|&&k|effects::ENABLED_KINDS.len()==1 || k!=self.last_item_kind)"),
        ("if k==self.last_item_kind {continue;}",
         "if effects::ENABLED_KINDS.len()>1 && k==self.last_item_kind {continue;}"),
    ]:
        if source.count(old) != 1:
            raise ValueError(f"Expected one spawn exclusion hook: {old}")
        source = source.replace(old, new)
    return source
