# SPDX-License-Identifier: GPL-3.0-or-later
import unittest
from pathlib import Path
from spawn_fixture import enabled_kinds, singleton_items


class SpawnFixtureTests(unittest.TestCase):
    def test_existing_expanded_and_future_spawn_sets(self):
        for kinds in ["Surge, EffectKind::Magnet, EffectKind::Phase",
                      "Surge, EffectKind::Magnet, EffectKind::Phase, EffectKind::Venom",
                      "Venom, EffectKind::Frost, EffectKind::Whirlpool"]:
            source = f"pub const ENABLED_KINDS: &[EffectKind] = &[EffectKind::{kinds}];"
            self.assertEqual(enabled_kinds(source, "Magnet"),
                             "pub const ENABLED_KINDS: &[EffectKind] = &[EffectKind::Magnet];")
        current = (Path(__file__).parents[2] / "src/world/effects/mod.rs").read_text()
        self.assertIn("= &[EffectKind::Magnet];", enabled_kinds(current, "Magnet"))

    def test_missing_or_duplicate_declarations_fail_loudly(self):
        declaration = "pub const ENABLED_KINDS: &[EffectKind] = &[];"
        for source in ["", declaration * 2]:
            with self.assertRaises(ValueError):
                enabled_kinds(source, "Magnet")

    def test_singleton_exclusion_updates_both_hooks_and_rejects_drift(self):
        source = (Path(__file__).parents[2] / "src/world/items.rs").read_text()
        fixture = singleton_items(source)
        self.assertIn("effects::ENABLED_KINDS.len()==1 ||", fixture)
        self.assertIn("effects::ENABLED_KINDS.len()>1 &&", fixture)
        with self.assertRaises(ValueError):
            singleton_items(fixture)


if __name__ == "__main__":
    unittest.main()
