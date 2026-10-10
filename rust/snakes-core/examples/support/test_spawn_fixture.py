# SPDX-License-Identifier: GPL-3.0-or-later
import unittest
import sys
from pathlib import Path
# Permit the documented direct invocation under Python's isolated (-I) mode.
sys.path.insert(0, str(Path(__file__).resolve().parent))
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

    def test_singleton_exclusion_updates_shared_policy_and_preserves_other_rules(self):
        source = (Path(__file__).parents[2] / "src/world/items.rs").read_text()
        fixture = singleton_items(source)
        self.assertEqual(fixture, source.replace("let exclude_repeat = true;",
                         "let exclude_repeat = effects::ENABLED_KINDS.len() > 1;"))
        self.assertIn("(!exclude_repeat || k!=self.last_item_kind)", fixture)
        self.assertIn("(exclude_repeat && k==self.last_item_kind)", fixture)
        self.assertIn("(k!=EffectKind::Whirlpool || self.vortex().is_none())", fixture)
        self.assertIn("(k==EffectKind::Whirlpool && self.vortex().is_some())", fixture)

    def test_repeat_policy_tolerates_formatting_and_future_exclusions(self):
        source = """    // spawn-fixture: repeat-exclusion
    let exclude_repeat  =  true ;
    let allowed = !exclude_repeat || kind != last;
    if !allowed || blocked_by_future_item(kind) { continue; }
"""
        self.assertEqual(singleton_items(source), source.replace("true ;",
                         "effects::ENABLED_KINDS.len() > 1 ;"))

    def test_missing_duplicate_modified_and_reapplied_contracts_fail_clearly(self):
        source = "// spawn-fixture: repeat-exclusion\nlet exclude_repeat = true;"
        for invalid in ["", source * 2, source.replace("true", "false"),
                        source.replace("exclude_repeat", "other_policy"),
                        singleton_items(source)]:
            with self.subTest(source=invalid), self.assertRaisesRegex(ValueError, "items.rs"):
                singleton_items(invalid)


if __name__ == "__main__":
    unittest.main()
