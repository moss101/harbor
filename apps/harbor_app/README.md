# harbor_app — the Harbor shell

Flutter shell for iOS, Android, macOS and Windows over the Rust core
(`core/harbor_ffi`, driven from a background isolate by
`packages/harbor_native`). The UI mirrors presentation choices only; policy
and runtime truth stay in the core, and every surface renders live core
facts or an honest empty/degraded state — never sample data.

## Layout

```
lib/
├── main.dart                 HarborApp, AppState (+ persisted preferences),
│                             HarborSurface enum, destinations
├── services/
│   ├── harbor_service.dart   ChangeNotifier bridge to the core worker
│   └── preferences.dart      HarborPreferences + file / memory stores
├── shell/
│   ├── adaptive_shell.dart   compact / medium / expanded / wide-rail / full
│   ├── harbor_lens.dart      Trust Pulse · active model · background work ·
│   │                         recent runs · knowledge status
│   ├── command_palette.dart  ⌘K / Ctrl+K: surfaces, actions, skills
│   ├── keyboard.dart         intents + shortcut map (UX-038)
│   └── surface_stack.dart    state-preserving cross-fade between surfaces
├── surfaces/                 one file per product surface (+ work/ renderers)
├── widgets/                  trust + operation helpers shared by surfaces
└── l10n/                     app_en.arb / app_ar.arb → generated delegates
```

The design system (tokens, breakpoints, named patterns: Harbor Rail, Trust
Pulse, Model Dock, Run Trail, Harbor Sheet, Fit Score, Artifact Diff,
states, banners, progress) lives in `packages/harbor_ui`.

## Responsive contract (UI authority §4)

| Width        | Navigation                              | Lens                 |
|--------------|-----------------------------------------|----------------------|
| ≤ 599        | top bar + 5-item bottom bar (More sheet) | modal sheet          |
| 600–1023     | icon + label side rail                   | end drawer           |
| 1024–1179    | 72 px collapsed rail (tooltips)          | end drawer           |
| 1180–1279    | 220 px rail                              | end drawer           |
| ≥ 1280       | 220 px rail                              | docked 320 px (toggle) |

Desktop adds ⌘/Ctrl+1…9 (surfaces), ⌘K (palette), ⌘L (Lens), ⌘O (open
file), ⌘, (settings). Motion follows the token scale and collapses under
the platform's reduce-motion setting.

## Running

```
flutter run -d macos          # desktop
flutter run -d "iPhone 17 Pro"  # simulator (native core embedded by the
                                # "Embed Harbor Native Core" build phase)
flutter test                  # shell, accessibility and l10n gates
```

`flutter`/`dart` are not on PATH on the qualification machine; use
`~/harbor-tools/flutter/bin/...`. Tests that touch the core need
`core/target/debug/libharbor_ffi.dylib` (`cargo build -p harbor_ffi`).

## Gates that every UI change must keep green

- `dart format --set-exit-if-changed lib test`, `flutter analyze`
- `test/shell_test.dart` — layouts at 390 / 800 / 1100 / 1280 / 1440,
  RTL mirroring, persisted preferences, palette + shortcuts, and the live
  core journeys (composer → durable run, workbook grid, skills, knowledge,
  grounded generation)
- `test/accessibility_audit_test.dart` — every control labeled, 44 px
  targets, keyboard traversal, and no overflow at 200 % text on 320 / 390
  / 1280 px (failures name the offending widget)
- `test/l10n_coverage_test.dart` — EN/AR key parity and no hardcoded prose
  in `lib/surfaces`, `lib/shell`, `lib/widgets`
