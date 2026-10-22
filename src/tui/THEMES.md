# Colour themes

fmn includes all 37 built-in kqt themes, plus its original Mint, Rose, and Iris
palettes. Mint remains the default. Config names match kqt exactly.

Press Ctrl-T and type to fuzzy search names, ignoring case and accents. Config-style
hyphens work too: `tokyo-night` finds Tokyo Night, and `rpd` finds Rosé Pine Dawn.
Matching letters are highlighted. Backspace edits the filter and Ctrl-U clears
it. The filter is separate from command-history search and resets each time the
picker opens. Preview matches with Up/Down, Page Up/Down, Home/End, or the mouse
wheel. The list scrolls to keep the preview visible. Enter or a click applies
the theme and saves it as the default in fmn's config file. A missing file is
created automatically; existing settings and comments are preserved. The save
honours `FMN_CONFIG` and the normal XDG config location. Escape restores the
previous theme without saving. You can also set the default directly:

```toml
[ui]
theme = "catppuccin-mocha"
```

| Family | Config names |
| --- | --- |
| fmn originals | `mint`, `rose`, `iris` |
| Terminal | `terminal`, `no-color` |
| Accessible | `accessible-dark`, `accessible-light` |
| Nord | `nord` |
| Rosé Pine | `rose-pine`, `rose-pine-moon`, `rose-pine-dawn` |
| Catppuccin | `catppuccin-mocha`, `catppuccin-macchiato`, `catppuccin-frappe`, `catppuccin-latte` |
| Editor palettes | `dracula`, `material`, `palenight`, `darcula`, `monokai` |
| Ubuntu and Lemonade | `ubuntu`, `lemonade`, `pink-lemonade` |
| Gruvbox | `gruvbox-dark`, `gruvbox-light` |
| Tokyo Night | `tokyo-night`, `tokyo-night-storm`, `tokyo-night-light` |
| One Dark | `one-dark` |
| Solarized | `solarized-dark`, `solarized-light` |
| Ayu | `ayu-dark`, `ayu-mirage`, `ayu-light` |
| Everforest dark | `everforest-dark-hard`, `everforest-dark-medium`, `everforest-dark-soft` |
| Everforest light | `everforest-light-hard`, `everforest-light-medium`, `everforest-light-soft` |

`terminal` follows the terminal's default background/text and ANSI palette.
`no-color` uses terminal-default colours throughout, including theme-picker
swatches, with reverse-video selections and bold/underlined search matches.
A nonempty `NO_COLOR` starts fmn in `no-color` mode.

The accessible presets preserve kqt's contrasting selection foregrounds.
Selected command matches, metadata, status symbols, and menu labels use that
foreground so they remain readable against the selection fill. Normal/muted,
accent and status text target 4.5:1 contrast on the main and panel backgrounds;
selection text targets 4.5:1 on its fill. Borders and selection fills target 3:1
against adjacent backgrounds. Colour is accompanied by selection markers,
underlined matches, and exit-status symbols. Terminal settings can affect
appearance; these targets do not claim formal accessibility conformance.

## Palette attribution

The shared semantic palette values come from
[kqt's built-in catalogue](https://github.com/z0mbix/kqt/blob/d81043e8a03cf3f680040ec27394ed81a6c56216/internal/tui/themes.go).
fmn maps kqt's surface to its panels and accent to search matches; its
interface has no separate warning-colour token. The original Mint, Rose, and
Iris values are retained. Only palette values are adapted, with the upstream
sources and notices preserved below.

| Palette | Source | License copy |
| --- | --- | --- |
| Nord | https://github.com/nordtheme/nord | [MIT](licenses/nord.txt) |
| Rosé Pine | https://github.com/rose-pine/rose-pine-theme | [MIT](licenses/rose-pine.txt) |
| Catppuccin | https://github.com/catppuccin/catppuccin | [MIT](licenses/catppuccin.txt) |
| Dracula | https://github.com/dracula/dracula-theme | [MIT](licenses/dracula.txt) |
| Material (Default) | [material.vim](https://github.com/kaicataldo/material.vim/blob/32f423c825ee89a37d66e3d8f00e777c7d8a41ab/colors/material.vim) | [MIT](licenses/material.txt) |
| Palenight | [Palenight Theme for VS Code](https://github.com/whizkydee/vscode-palenight-theme/blob/63949af027921fd09466c1e6003bd2b994e14a9e/themes/palenight.json) | [MIT](licenses/palenight.txt) |
| Darcula | [Darcula for VS Code](https://github.com/rokoroku/vscode-theme-darcula/blob/657baba5dc1d632daefe8e6130abcca550f0ea1f/themes/darcula.json) | [MIT](licenses/darcula.txt) |
| Monokai | [VS Code's built-in Monokai](https://github.com/microsoft/vscode/blob/f20a64b96be4f2b10d7cd2703efb2fc85afaac18/extensions/theme-monokai/themes/monokai-color-theme.json) | [MIT](licenses/monokai.txt) |
| Ubuntu | [Radu Matei's Ubuntu Color Theme for VS Code](https://github.com/radu-matei/vscode-ubuntu-color-theme/blob/dbe2e82cfdda6ffb1e0cbe97b4cda96fcf18dda9/themes/ubuntu-color-theme.json) | [MIT](licenses/ubuntu.txt) |
| Lemonade | [Denner's Lemonade for VS Code](https://github.com/dennerrondinely/lemonade/blob/320c9e6ae95bf6478804ca85a75897b11156a536/themes/lemonade-color-theme.json) | [MIT](licenses/lemonade.txt) |
| Pink Lemonade (Light) | [Casseopeian's VS Code theme](https://github.com/gbmyt/pinkvscodetheme/tree/e620f7ac095092c8236181630a00d6e064ceb5f2) | [Attribution and upstream license status](licenses/pink-lemonade.txt) |
| Gruvbox | https://github.com/morhetz/gruvbox | [MIT/X11](licenses/gruvbox.txt) |
| Tokyo Night | https://github.com/folke/tokyonight.nvim | [Apache-2.0](licenses/tokyo-night.txt) |
| One Dark | https://github.com/atom/one-dark-syntax | [MIT](licenses/one-dark.txt) |
| Solarized | https://github.com/altercation/solarized | [MIT](licenses/solarized.txt) |
| Ayu (Dark, Mirage, Light) | [ayu-colors](https://github.com/ayu-theme/ayu-colors/tree/b0fd979a1ddf050101b43311fa598a1a9c5f1bbc) | [MIT](licenses/ayu.txt) |
| Everforest (Dark/Light, Hard/Medium/Soft) | [Everforest palette](https://github.com/sainnhe/everforest/blob/85a86eb62409e3ec88713bff3d1b9d7374e112e4/palette.md) | [MIT](licenses/everforest.txt) |

Ayu's translucent selection backgrounds are composited into opaque terminal
colours. Muted/foreground and status/accent colours are adjusted where needed
for readable contrast. Everforest retains all six background variants;
selection uses the corresponding popup background.

Material uses its original blue-grey background and cyan accent; Palenight
uses the purple accent and composited editor selection from its VS Code theme.
Darcula and Monokai retain their editor and panel backgrounds with brighter
muted/status text where needed. Ubuntu uses Radu Matei's aubergine backgrounds
and orange accent, with an opaque composite of the editor selection. Lemonade
uses Denner's charcoal backgrounds, turquoise accent, mint strings, and yellow
numbers. Pink Lemonade preserves its light pink editor,
sidebar, and selection backgrounds; text and status colours are darkened for
terminal readability. Its upstream repository supplies no explicit license;
only its palette values are referenced here, with attribution retained.

Retain these notices when distributing the bundled palettes.


The accessible presets are kqt designs informed by
[Okabe and Ito's Color Universal Design guidance](https://jfly.uni-koeln.de/color/).
Third-party notices are copied from kqt alongside these palettes. Release
archives include this guide and the `licenses/` directory; retain them when
redistributing the palettes.
