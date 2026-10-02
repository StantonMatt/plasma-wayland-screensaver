# Debian build test scheduling audit (0.7.0)

The Debian/Launchpad build enables all 13 CTest entries in `tests/CMakeLists.txt`.
Only the three real-time cadence tests listed below are skipped by
`patches/skip-shared-builder-cadence-checks.patch`. The upstream source and
ordinary CI build keep them enabled. Data rows share their test's decision.

No simulation speed, work-count, allocation, parity, or numeric tolerance is
relaxed. Tests with deterministic time inputs stay enabled. Ordinary event-loop
waits use readiness predicates or generous hang guards; they do not assert CPU
speed. Qt's `QBENCHMARK` measurements have no pass/fail timing thresholds.

## C++ and QML tests

### `tests/test_animationstate.cpp`

| Test | Decision |
| --- | --- |
| `respectsSteppedMonitorUnion` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `buildsConfiguredBallSet` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `activePhysicsStaysInsideSteppedDesktop` | Robustness fix: await 15 actual frameChanged signals (default 5 s guard), rather than assuming physics progressed after a 250 ms sleep; retain the containment checks. |
| `externalClockDrivesSeamlessPhysicsExclusively` | Keep: 30 ms wait asserts no autonomous updates, then explicit advance asserts exactly one update. Scheduler delay cannot create an expected timer that is disabled. |

### `tests/test_configuration.cpp`

| Test | Decision |
| --- | --- |
| `defaultsAndValidation` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `roundTrip` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `appliesSettingsAtomically` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `acceptsAllBundledVisualModules` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `supportsExpandedAndAutomaticFrameRates` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `migratesCombinedBlackVisual` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |

### `tests/test_fireflyrenderer.cpp`

| Test | Decision |
| --- | --- |
| `createsOneBatchedGeometryNode` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `reusesGeometryAcrossFrames` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `benchmarkThreeMonitorFrameAssembly` | Keep: QBENCHMARK reports timings only; no speed threshold. |

### `tests/test_overlaysnakes.cpp`

| Test | Decision |
| --- | --- |
| `worldFollowsOverlayConfigure` | Keep all monitor rows: QTRY_COMPARE awaits viewport changes (default 5 s guard). Reduced-motion settings disable manager clocks, so manual stepping stays deterministic during event processing. |
| `sharedArenaUsesAllOverlayViewports` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `sharedWorldSurvivesDriverRemoval` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `independentWorldDestroyedWithView` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |

### `tests/test_presentationclock.cpp`

| Test | Decision |
| --- | --- |
| `steadyCadence` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `oddRefreshUsesNearestVsync` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `lateSwapSkipsMissedSlots` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `queuedCallbackStallSkipsPastVsyncs` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `swapJitterDoesNotAccumulate` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `timerDeadlinesStayAbsolute` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `submissionLatencyDoesNotMovePhase` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `delayedSubmissionDoesNotAccumulate` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `clockUsesFixedPhase` | Debian-only skip: real timers must yield a median interval within 1 ms of target (both data rows). Deterministic submissionLatencyDoesNotMovePhase and delayedSubmissionDoesNotAccumulate remain enabled. |
| `refreshChangesAndFeedbackRecovery` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `invalidRefreshFallsBackTo60` | Keep: deterministic simulated timestamps; no real-time assertion. |
| `synchronousWorkDoesNotExtendTimerPeriod` | Debian-only skip: seven real 33.3 ms periods plus synchronous sleeps must finish in 210–285 ms. Deterministic timerDeadlinesStayAbsolute remains enabled. |
| `stopResumeAndSharedListeners` | Robustness fix: retain stop/resume and identical-listener assertions; compare the synchronous bootstrap delta with one nominal period plus measured resume-call duration instead of a fixed 30–50 ms window. Await initial progress with Qt’s default 5 s guard. |
| `renderedFeedbackIncludesStallTime` | Debian-only skip: real Qt Quick swaps require a 29–39 ms median and sufficient sub-100 ms samples. Deterministic lateSwapSkipsMissedSlots, queuedCallbackStallSkipsPastVsyncs and feedback-recovery tests retain stall coverage. |

### `tests/test_snakerenderer.cpp`

| Test | Decision |
| --- | --- |
| `retainedGeometrySurvivesGrowthShrinkAndMalformedPrimitive` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `resyncReadsNativePreviousPositions` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `denseFoodDetailUsesHysteresis` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `developerSteeringArrowAndViewportScaling` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `sharedNativeFrameAndDestruction` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `abiLimitedArenaMapsToEveryViewport` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `benchmarkMatureGeometry` | Keep: QBENCHMARK reports timings only; no speed threshold. |
| `benchmarkMatureSyncFrame` | Keep: QBENCHMARK reports timings only; no speed threshold. |

### `tests/test_snakescore_abi.cpp`

| Test | Decision |
| --- | --- |
| `abiVersion` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |

### `tests/test_snakesimulation.cpp`

| Test | Decision |
| --- | --- |
| `configMapping` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `fixedStepAccumulatorClampAndPause` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `logicalExtentSurvivesAbiLimitAndSettings` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `resizeAndReconfigurePreserveState` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `sharedPresentationDoesNotMultiplyStepsAndSurvivesViewRemoval` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `exportBuffersRetainHighWaterCapacity` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |

### `tests/test_statemachine.cpp`

| Test | Decision |
| --- | --- |
| `idleActivationAndDismissal` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `previewAndFailure` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |
| `ignoresDuplicateTriggers` | Keep: synchronous state/geometry/ABI or explicitly stepped simulation; no wall-clock assertion. |

### `tests/qml/tst_snakes.qml`

| Test | Decision |
| --- | --- |
| `test_loadsNativeView` | Robustness fix: waitForRendering with a 5 s guard after requesting a renderer update replaces a fixed 50 ms wait; retain native renderer and captured-image dimension checks. |
| `test_nativePixels` | Robustness fix: waitForRendering with a 5 s guard after requesting a renderer update replaces a fixed 50 ms wait; retain pixel check. Existing software-backend skip remains because custom geometry is unsupported there. |
| `test_modePreservesManagerViewportOffset` | Keep: synchronous property changes; no wall-clock dependency. |

### `tests/qml/tst_visual_resources.qml`

| Test | Decision |
| --- | --- |
| `test_firefliesUseSceneGraphNodes` | Keep: synchronous resource/node checks after windowShown; no cadence assertion. |

`tests/test_snakesqml.cpp` supplies a simulation advanced by exactly one explicit
1/30 s step; it has no independent timer assertions. QML tests start only after
`windowShown`.

## Cargo tests (`snakes-core-rust`)

Cargo runs `test --frozen` in the release profile for Debian. The allocation
integration binary includes the library source, so the unit tests below run
again in that binary. All remain enabled. AI deadlines, forecast horizons,
respawn times, cache budgets, and profiling work limits are simulation ticks or
operation counts, independent of host scheduling. `Instant` is used only for
printed allocation-test diagnostics and optional AI profiling, never a timing
assertion. The crate has no
doc-test examples with timing expectations. C smoke files and Python golden
fixture generation are support tools, not tests invoked by Cargo/CTest.

### `rust/snakes-core/src/ai/spatial.rs`

| Test | Decision |
| --- | --- |
| `rectangular_queries_cover_unique_buckets_on_both_axes` | Keep: exhaustive finite grid/coverage and work-cap assertions; no wall-clock dependency. |
| `discovery_rings_cover_every_cell_once_including_even_extents` | Keep: exhaustive finite grid/coverage and work-cap assertions; no wall-clock dependency. |
| `local_clearance_cluster_and_cardinal_neighbours_are_unique` | Keep: exhaustive finite grid/coverage and work-cap assertions; no wall-clock dependency. |
| `small_grid_area_and_waypoints_keep_their_coverage_and_caps` | Keep: exhaustive finite grid/coverage and work-cap assertions; no wall-clock dependency. |

### `rust/snakes-core/src/ai/tests.rs`

| Test | Decision |
| --- | --- |
| `wrapped_food_discovery_shortlists_each_food_once` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `wrapped_safety_and_cache_charge_each_body_record_once` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `avoids_losing_head_on_and_crossing_body` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `turns_early_for_walls_but_wraps_without_an_artificial_wall` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `capped_fill_reports_uncertainty_and_respects_own_body_rule` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `safety_checks_toroidal_body_across_the_seam` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `same_seed_repeats_and_query_does_not_consume_world_randomness` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `smallest_and_largest_valid_arenas_do_not_panic` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `size_advantage_can_win_a_food_contest_by_head_collision_or_cutoff` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `continuation_honors_the_turn_then_straight_deadline` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `diagnostic_snapshot_replays_mechanics_and_randomness_exactly` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `area_cache_is_invalidated_on_world_rebuild` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `cached_body_queries_match_uncached_and_preserve_work_limits` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `turning_space_releases_tails_and_respects_the_self_collision_switch` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `sustained_turn_escape_interrupts_tail_recovery` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `diagnostics_and_profiling_do_not_change_decisions` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `first_step_broad_phase_includes_observed_body_motion` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `size_advantage_accepts_a_winning_immediate_head_contact` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `growth_release_delay_uses_stretch_cycles_and_the_segment_cap` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `tail_recovery_ends_when_current_space_is_open` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `truncated_turn_plan_keeps_its_intended_exit_heading` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `rival_forecast_honors_a_known_turn_then_straight_plan` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |

### `rust/snakes-core/src/world/tests.rs`

| Test | Decision |
| --- | --- |
| `initial_population_and_clearance` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `food_expiry_replenishes_at_most_three_and_bounces` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `body_replays_l_shaped_head_path` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `growth_inserts_neck_and_preserves_previous_correspondence` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `vacuum_capture_lock_release_and_growth_storage` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `wrap_spacing_vacuum_and_narrow_final_bin` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `head_on_size_and_less_than_four_tie_rule` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `swept_head_on_and_body_hits_neck_included` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `self_collision_excludes_first_ten_segments` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `maturity_turn_radius_length_penalty_rush_and_radius_cap` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `per_snake_and_world_caps_block_growth_not_motion` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `death_burst_cap_preserves_nutrition_and_locked_food` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `respawn_reuses_id_increments_generation_and_checks_clearance` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `resize_scales_current_previous_trail_food_velocity_and_targets` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `ring_pruning_and_wraparound_preserve_placement` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `invalid_controls_do_not_poison_world_and_reconfigure_count_resets` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `cap_fixture_is_finite` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `density_shrink_and_grow_do_not_reuse_generation` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `toroidal_segment_math_and_touching_intersections` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `larger_snakes_create_more_death_food` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |

### `rust/snakes-core/src/world/golden.rs`

| Test | Decision |
| --- | --- |
| `rng_matches_js_f64_seed_and_lcg` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `scripted_mechanics_match_unmodified_javascript` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |

### `rust/snakes-core/tests/ai_ffi.rs`

| Test | Decision |
| --- | --- |
| `default_ai_debug_and_scripted_switch_preserve_the_abi` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |

### `rust/snakes-core/tests/allocation.rs`

| Test | Decision |
| --- | --- |
| `zero_allocations_at_caps_including_death_and_respawn` | Keep: thread-local allocation count must be zero. Fixed simulation ticks; AI elapsed timings are printed only. |
| `zero_allocations_with_ai_at_caps_and_after_reconfiguration` | Keep: thread-local allocation count must be zero. Fixed simulation ticks; AI elapsed timings are printed only. |

### `rust/snakes-core/tests/controllers.rs`

| Test | Decision |
| --- | --- |
| `recorded_table_matches_function_and_generation_filter` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |

### `rust/snakes-core/tests/ffi.rs`

| Test | Decision |
| --- | --- |
| `ffi_round_trip_and_validation` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |
| `ffi_layout_sizes_and_offsets` | Keep: deterministic seeded state, fixed ticks, numeric or operation-count assertions; no wall-clock threshold. |

## Parity tests

| Test / fixture | Decision |
| --- | --- |
| `snakes-parity`: `body-hit` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `deadly-walls` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `death-burst-food` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `default-wrap` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `feeding-growth-insertion` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `head-on-equal` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `head-on-length-difference` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `maximum-density-trails` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `respawn` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `self-collisions` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `self-exclusion` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `vacuum-lock` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `world-resize` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `wrap-seam-collision` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity`: `wrap-seam-crossing` | Keep: fixed recorded ticks, exact discrete outcomes and 1e-6 numeric tolerance. No speed assertion. |
| `snakes-parity-record`: `test_record` (all 15 fixtures above) | Keep: simulation driver disabled, reduced motion enabled, explicit fixed steps. Regenerated fixtures must be byte-identical. |

The parity subprocess's 60 s per-fixture timeout, CTest's 120 s replay timeout,
and the recorder's 180 s process / 240 s CTest timeouts are hang guards, not
real-time headroom checks. They cover finite, deterministic workloads and remain
unchanged. `test_compare.py` is a deterministic comparator unit-test tool, not
registered as a Debian CTest entry. Rust benchmark/scorecard examples are also
not executed by Cargo test (the parity example alone is explicitly built and
replayed).

ECM also registers `appstreamtest`: keep the AppStream metadata validator; it
checks schema/content without a wall-clock assertion (14 total CTest entries).
