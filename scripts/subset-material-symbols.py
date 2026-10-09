#!/usr/bin/env python3
"""Regenerate src/styles/fonts/material-symbols-outlined.woff2 as a subset
containing only the glyphs this app actually references.

Why this exists: the upstream `material-symbols` package ships one variable
font with ~4000 icons (~3.9 MB). This app's `<Ms>` component renders icons by
typing their name as a ligature (e.g. "settings"), so a naive `pyftsubset
--text` pass can't discriminate between icons - nearly every name shares the
same a-z/underscore alphabet, so the ligature-closure algorithm conservatively
keeps almost the whole font. This script instead prunes the GSUB ligature
table itself down to only the needed names *before* subsetting, which lets
pyftsubset correctly drop the other ~3900 icons' outlines and variable-font
deltas too. Result: ~90 KB instead of ~3.9 MB.

Run this whenever a new icon name is added to the app (a build that renders
an icon not in the frozen subset shows the literal typed text instead of a
glyph - there is no error, so this is easy to miss).

Prerequisites: `pip install fonttools`, and `node_modules/material-symbols`
present (`npm ci`).
"""

import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SRC = REPO_ROOT / "src"
UPSTREAM_FONT = REPO_ROOT / "node_modules/material-symbols/material-symbols-outlined.woff2"
OUTPUT_FONT = SRC / "styles/fonts/material-symbols-outlined.woff2"

# Icon names reachable only through a plain string array (ICON_CHOICES) or a
# lookup table (CHANNEL_ICONS's fallback values) rather than an `icon=`/
# `icon:`/`<Ms name=` literal - the regex scan below can't see these, so they
# are listed by hand. Keep in sync with src/components/Icons.tsx.
ALWAYS_INCLUDE = {
    # Dynamic update status icons.
    "download", "check_circle", "error",
    # ICON_CHOICES (channel icon picker)
    "sports_esports", "forum", "music_note", "desktop_windows", "headphones",
    "mic", "movie", "tv", "videogame_asset", "campaign", "record_voice_over",
    "radio", "podcasts", "terminal", "public", "star",
    # CHANNEL_ICONS fallback values + channelIcon()'s own default
    "cable", "graphic_eq",
    # OSD_POSITION_ICONS (Settings > Popup Overlay > Position picker), in
    # SettingsScreen.tsx
    "north_east", "east", "south_east", "north_west", "west", "south_west",
}

NAME_ATTR_RE = re.compile(r"<Ms\s+name=\{([^}]+)\}|<Ms\s+name=\"([^\"]+)\"", re.DOTALL)
ICON_PROP_RE = re.compile(r'\bicon\s*[:=]\s*"([a-zA-Z_][a-zA-Z0-9_]*)"')
STRING_LITERAL_RE = re.compile(r'"([a-zA-Z_][a-zA-Z0-9_]*)"')


def discover_icon_names() -> set[str]:
    names: set[str] = set(ALWAYS_INCLUDE)
    for path in SRC.rglob("*.tsx"):
        text = path.read_text(encoding="utf-8")
        for match in NAME_ATTR_RE.finditer(text):
            expr = match.group(1) or match.group(2) or ""
            names.update(STRING_LITERAL_RE.findall(expr) if match.group(1) else [expr])
        names.update(ICON_PROP_RE.findall(text))
    for path in SRC.rglob("*.ts"):
        text = path.read_text(encoding="utf-8")
        names.update(ICON_PROP_RE.findall(text))
    return names


def real_glyph_names(font) -> set[str]:
    return {g for g in font.getGlyphOrder() if not g.startswith(".") and "." not in g}


def main() -> int:
    try:
        from fontTools.ttLib import TTFont
    except ImportError:
        print("Missing dependency: pip install fonttools", file=sys.stderr)
        return 1

    if not UPSTREAM_FONT.exists():
        print(f"Missing {UPSTREAM_FONT} - run `npm install --no-save material-symbols` first "
              "(it is not a runtime dependency once this subset is committed).", file=sys.stderr)
        return 1

    candidates = discover_icon_names()
    font = TTFont(str(UPSTREAM_FONT))
    valid = real_glyph_names(font)
    target = sorted(candidates & valid)
    missing = sorted(n for n in candidates if n not in valid and n in ALWAYS_INCLUDE)
    if missing:
        print(f"warning: ALWAYS_INCLUDE names not found in the upstream font: {missing}", file=sys.stderr)
    unresolved = sorted(candidates - valid - set(missing))
    if unresolved:
        # Expected: incidental string matches (comparison values, CSS class
        # names, etc.) that happen to look like an icon name. Not an error,
        # just noise from the regex scan - printed so a genuinely-missing
        # icon name (typo, renamed upstream glyph) is easy to spot by eye.
        print(f"ignored non-glyph string matches: {unresolved}", file=sys.stderr)
    print(f"subsetting to {len(target)} glyphs")

    # Prune the GSUB ligature table to just the target glyphs *before*
    # subsetting - see the module docstring for why this ordering matters.
    gsub = font["GSUB"].table
    for lookup in gsub.LookupList.Lookup:
        for subtable in lookup.SubTable:
            ext = getattr(subtable, "ExtSubTable", subtable)
            ligatures = getattr(ext, "ligatures", None)
            if ligatures is None:
                continue
            pruned = {}
            for first_glyph, lig_list in ligatures.items():
                kept = [lig for lig in lig_list if lig.LigGlyph in target]
                if kept:
                    pruned[first_glyph] = kept
            ext.ligatures = pruned

    with tempfile.NamedTemporaryFile(suffix=".woff2", delete=False) as tmp:
        pruned_path = Path(tmp.name)
    font.save(str(pruned_path))

    text_path = pruned_path.with_suffix(".txt")
    text_path.write_text(" ".join(target), encoding="utf-8")

    OUTPUT_FONT.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            sys.executable, "-m", "fontTools.subset", str(pruned_path),
            f"--output-file={OUTPUT_FONT}",
            f"--text-file={text_path}",
            "--layout-features=*",
            "--flavor=woff2",
            "--no-hinting",
        ],
        check=True,
    )
    pruned_path.unlink(missing_ok=True)
    text_path.unlink(missing_ok=True)
    print(f"wrote {OUTPUT_FONT} ({OUTPUT_FONT.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
