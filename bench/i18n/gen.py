#!/usr/bin/env python3
"""README numbers and translations from one benchmark series.
   gen.py OUTDIR|--apply SHELLS.jsonl[,MORE.jsonl] COMPOSITORS.jsonl
OUTDIR: write README.md and README.<lang>.md there (dry run); --apply: write into the repository."""
import json, re, statistics, subprocess, sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).parent
ROOT = HERE.parent.parent
LANGS = ["pl", "de", "es", "zh-CN", "ja"]
COMMA = {"pl", "de", "es"}
med = statistics.median

EN = {
    "t_sh": "Shell | What it draws | Memory (PSS) | GPU memory | CPU idle | CPU active",
    "d_bar": "bar", "d_mistp": "bar, dock, island, desktop widgets, notifications, wallpaper",
    "d_mistl": "all of that, the wallpaper a live GLSL scene", "d_qs": "an empty bar",
    "d_noct": "bar, photo wallpaper (and its first-run dialog)",
    "n_mistp": "Mist, photo wallpaper", "n_mistl": "Mist, live wallpaper", "n_qs": "Quickshell 0.3, one empty `PanelWindow`",
    "t_co": "Compositor | Memory (PSS) | CPU, a client drawing on the CPU every frame | CPU, a fullscreen OpenGL client | CPU per OpenGL frame",
}
SHELL = {  # result name -> (label or key, "draws" key)
    "swaybar": ("swaybar (sway 1.12)", "d_bar"), "Waybar": ("Waybar 0.15", "d_bar"), "nwg-panel": ("nwg-panel 0.12", "d_bar"),
    "Ironbar": ("Ironbar 0.19", "d_bar"), "Noctalia": ("Noctalia 5.4", "d_noct"),
    "Quickshell, empty PanelWindow": ("n_qs", "d_qs"), "Mist": ("n_mistl", "d_mistl"), "Mist, photo wallpaper": ("n_mistp", "d_mistp"),
}
COMP = {"sway": "sway 1.12", "labwc": "labwc 0.20", "weston": "weston 15", "mist-comp": "mist-comp"}


def load(paths):
    rows = []
    for p in paths.split(","):
        new = [json.loads(l) for l in open(p) if l.strip()]
        key = "shell" if "shell" in new[0] else "comp"
        names = {d[key] for d in new}
        rows = [d for d in rows if d[key] not in names] + new
    return rows


def shells(rows):
    by = defaultdict(list)
    for d in rows:
        by[(d["shell"], d["scenario"])].append(d["total"])
    out = {}
    for s in {k[0] for k in by}:
        i, a = by[(s, "idle")], by[(s, "active")]
        out[s] = dict(pss=med(t["pss"] for t in i), gpu=med(t["gpu"] for t in i), cpui=med(t["cpu"] for t in i),
                      cpua=med(t["cpu"] for t in a), wake=med(t["wake"] for t in i), runs=len(i))
    return out


def comps(rows):
    by = defaultdict(list)
    for d in rows:
        by[(d["comp"], d["scenario"])].append(d)
    out = {}
    for c in {k[0] for k in by}:
        e = by[(c, "egl")]
        out[c] = dict(pss=med(d["total"]["pss"] for d in by[(c, "idle")]), shm=med(d["total"]["cpu"] for d in by[(c, "shm")]),
                      egl=med(d["total"]["cpu"] for d in e), frame=med(10 * d["total"]["cpu"] / d["fps"] for d in e if d.get("fps")),
                      idle=med(d["total"]["cpu"] for d in by[(c, "idle")]), runs=len(e))
    return out


def num(s, lang):
    return re.sub(r"(\d)\.(\d)", r"\1,\2", s) if lang in COMMA else s


def cpu(v):
    return "0 %" if v < 0.005 else "%.2f %%" % v


def shell_table(S, T, lang):
    lines = ["| " + T["t_sh"] + " |", "|---|---|---|---|---|---|"]
    for s in sorted(S, key=lambda s: S[s]["pss"]):
        name, draws = SHELL[s]
        name = T.get(name, name)
        v = S[s]
        pss = "%.0f MB" % v["pss"]
        if s.startswith("Mist"):
            name = f"**{name}**"
        if s == "Mist, photo wallpaper":
            pss = f"**{pss}**"
        gpu = "–" if v["gpu"] < 0.5 else "%.0f MiB" % v["gpu"]
        lines.append(f"| {name} | {T[draws]} | {pss} | {gpu} | {num(cpu(v['cpui']), lang)} | {num(cpu(v['cpua']), lang)} |")
    return "\n".join(lines)


def comp_table(C, T, lang):
    lines = ["| " + T["t_co"] + " |", "|---|---|---|---|---|"]
    best = {k: min(C, key=lambda c: C[c][k]) for k in ("shm", "egl", "frame")}
    b = lambda c, k, s: f"**{s}**" if best[k] == c else s
    for c in sorted(C, key=lambda c: C[c]["shm"]):
        v = C[c]
        name = "**mist-comp**" if c == "mist-comp" else COMP[c]
        fr = ("%.2f ms" if v["frame"] < 0.5 else "%.1f ms") % v["frame"]
        row = f"| {v['pss']:.0f} MB | {b(c, 'shm', '%.1f %%' % v['shm'])} | {b(c, 'egl', '%.1f %%' % v['egl'])} | {b(c, 'frame', fr)} |"
        lines.append(f"| {name} " + num(row, lang))
    return "\n".join(lines)


def slug(h):
    return re.sub(r"\s", "-", re.sub(r"[^\w\s-]", "", h.lower()))


def parse(path):
    d, k = {}, None
    for line in open(path, encoding="utf-8").read().split("\n"):
        if line.startswith("@@"):
            k = line[2:].strip()
            d[k] = []
        elif k:
            d[k].append(line)
    return {k: "\n".join(v).rstrip("\n") for k, v in d.items()}


def main():
    dest, sp, cp = sys.argv[1:4]
    S, C = shells(load(sp)), comps(load(cp))
    for s in sorted(S, key=lambda s: S[s]["pss"]):
        print("  %-32s pss %6.1f gpu %3.0f cpu %.3f / %.3f wake %5.1f runs %d" % (s, S[s]["pss"], S[s]["gpu"], S[s]["cpui"], S[s]["cpua"], S[s]["wake"], S[s]["runs"]))
    for c in C:
        print("  %-10s pss %6.1f idle %.3f shm %.2f egl %.2f frame %.3f runs %d" % (c, C[c]["pss"], C[c]["idle"], C[c]["shm"], C[c]["egl"], C[c]["frame"], C[c]["runs"]))
    P, L = S["Mist, photo wallpaper"], S["Mist"]
    vals = dict(wake="%.0f" % P["wake"], p_pss="%.0f" % P["pss"], l_pss="%.0f" % L["pss"],
                lo="%.0f" % S["swaybar"]["pss"], hi="%.0f" % max(v["pss"] for v in S.values()))
    apply = dest == "--apply"
    out = None if apply else Path(dest)
    if out:
        out.mkdir(parents=True, exist_ok=True)

    # English: tokens, the chart's alt text, both tables
    en = (ROOT / "README.md").read_text(encoding="utf-8")
    en = en.replace("@P_WAKE@", vals["wake"])
    en = re.sub(r'alt="Idle memory \(PSS\)[^"]*"',
                f'alt="Idle memory (PSS) of each shell alone, from swaybar at {vals["lo"]} MB to Noctalia at {vals["hi"]} MB: '
                f'Mist with a photo wallpaper {vals["p_pss"]} MB, Mist with the live wallpaper {vals["l_pss"]} MB"', en)
    en, n1 = re.subn(r"\| Shell \| What it draws[^\n]*\n(\|[^\n]*\n)+", shell_table(S, EN, "en") + "\n", en)
    en, n2 = re.subn(r"\| Compositor \| Memory \(PSS\)[^\n]*\n(\|[^\n]*\n)+", comp_table(C, EN, "en") + "\n", en)
    assert n1 == 1 and n2 == 1, (n1, n2)
    assert "@" not in re.sub(r"https?://\S+|[\w.]+@[\w.]+", "", en) or not re.search(r"@[A-Z_]+@", en), "token left"
    (ROOT / "README.md" if apply else out / "README.md").write_text(en, encoding="utf-8")

    skel = (HERE / "skel.md").read_text(encoding="utf-8")
    for lang in LANGS:
        T = parse(HERE / f"{lang}.txt")
        old = subprocess.run(["git", "-C", str(ROOT), "show", f"HEAD:docs/i18n/README.{lang}.md"], capture_output=True, text=True, check=True).stdout.split("\n")
        badges = [l for l in old[:14] if l.startswith("[![")]
        assert len(badges) == 4, (lang, badges)
        T["BADGES"] = "\n".join(badges)
        for a, h in (("a_what", "h_what"), ("a_why", "h_why"), ("a_install", "h_install"), ("a_keys", "h_keys"), ("a_dev", "h_dev"), ("a_try", "h_try")):
            T[a] = slug(T[h])
        T["SHELLS"], T["COMPS"] = shell_table(S, T, lang), comp_table(C, T, lang)
        text = skel
        for _ in range(2):  # values may hold {{a_try}}
            text = re.sub(r"\{\{(\w+)\}\}", lambda m: T[m.group(1)], text)
        for k, v in vals.items():
            text = text.replace("{" + k + "}", v)
        left = re.findall(r"\{\{?\w+\}?\}", text)
        assert not left, (lang, left)
        (ROOT / "docs/i18n" / f"README.{lang}.md" if apply else out / f"README.{lang}.md").write_text(text, encoding="utf-8")
        print("  wrote", lang, len(text.split("\n")), "lines")


main()
