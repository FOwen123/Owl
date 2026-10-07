import re
BODY = "M16 21 L13 7 L26 15 Q32 13.5 38 15 L51 7 L48 21 Q56 31 54 44 Q51 60 32 60 Q13 60 10 44 Q8 31 16 21 Z"
BEAK = "M28.5 42 L35.5 42 L32 48.5 Z"
SLITS = "M15.5 33 Q23 39.5 30.5 33 Q23 36.5 15.5 33 Z M33.5 33 Q41 39.5 48.5 33 Q41 36.5 33.5 33 Z"
def circ(cx, cy, r): return f"M{cx-r} {cy} a{r} {r} 0 1 0 {2*r} 0 a{r} {r} 0 1 0 {-2*r} 0 Z"
EYES = circ(23,33,8) + circ(41,33,8)
PUPILS = circ(23,33.5,3.4) + circ(41,33.5,3.4)
def scale(d, s, t):  # absolute-command paths only; arcs ('a') scale radii and deltas
    out, toks = [], re.findall(r"[A-Za-z]|-?\d+\.?\d*", d)
    cmd, i = None, 0
    for tok in toks:
        if tok.isalpha(): cmd, i = tok, 0; out.append(tok); continue
        v = float(tok)
        if cmd == "a":
            if (i % 7) in (2, 3, 4): out.append(tok); i += 1; continue
            v = v * s
        else:
            v = v * s + t
        out.append(f"{v:.2f}"); i += 1
    return " ".join(out)
def svg(body): return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">{body}</svg>'
def owl(c): return f'<path fill-rule="evenodd" fill="{c}" d="{BODY}{EYES}{BEAK}"/><path fill="{c}" d="{PUPILS}"/>'
def sleepy(c): return f'<path fill-rule="evenodd" fill="{c}" d="{BODY}{SLITS}{BEAK}"/>'
def badge(c):
    s, t = 0.72, 32 * 0.28
    return (f'<path fill-rule="evenodd" fill="{c}" d="{circ(32,32,31)} {scale(BODY,s,t)}"/>'
            f'<path fill-rule="evenodd" fill="{c}" d="{scale(EYES,s,t)} {scale(PUPILS,s,t)} {scale(BEAK,s,t)}"/>')
def warn(c):
    s = 0.84
    return (f'<path fill-rule="evenodd" fill="{c}" d="{scale(BODY+EYES+BEAK,s,0)}"/><path fill="{c}" d="{scale(PUPILS,s,0)}"/>'
            f'<path fill-rule="evenodd" fill="{c}" d="{circ(49,49,14)} M47 39 H51 V52 H47 Z M47 55 H51 V59 H47 Z"/>')
out = {"tray_idle": owl("#fff"), "tray_idle_dark": owl("#000"),
 "tray_recording": badge("#fff"), "tray_recording_dark": badge("#000"),
 "tray_transcribing": sleepy("#fff"), "tray_transcribing_dark": sleepy("#000"),
 "tray_idle_warning": warn("#fff"), "tray_idle_warning_dark": warn("#000"),
 "handy": owl("#4c8dff"), "recording": badge("#4c8dff"), "transcribing": sleepy("#4c8dff"), "handy_warning": warn("#4c8dff")}
for k, v in out.items(): open(f"{k}.svg", "w").write(svg(v))
