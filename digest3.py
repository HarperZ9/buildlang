import json, io
L = [x for x in json.load(io.open("H:/x-likes-2026-11-12/likes-pass3.json", encoding="utf8"))["likes"] if not x["known"]]
def row(x):
    h = next((p for p in (x["names"][0] if x["names"] else "").split(" | ") if p.startswith("@")), "?")
    t = " ".join(x["texts"][:1]).replace("\n", " ").replace("|", "/").strip()
    t = (t[:110] + "...") if len(t) > 110 else t
    m = ("video " if x["hasVideo"] else "") + ((str(len(x["imgs"])) + " img") if x["imgs"] else "")
    return "- [%s](%s) %s%s" % (h, x["url"], t or "(no text)", (" [" + m.strip() + "]") if m.strip() else "")
G = [
 ("Skeuomorphism and interface", "This is the post you meant. @mutableos argues that skeuomorphism failed from too little realism: an app that draws stitching, leather, brass or glass should give those things physical properties, and it gave untouchable pixels. A reply points at audio hardware as the place where that interface already works. The two videos show it done. The Stripe piece is a rack of counters, meters, seven-segment displays and a ring-bound manual whose pages turn, all carrying live transaction figures. The Tubik piece is an archive whose navigation is a stack of coloured file tabs that open into folders, paper slips, clipped receipts and a bench instrument. For us: the physics DAW and the element bench already hold materials with measured behaviour, so our controls can respond as the material would (weight, spring, detent, wear), which is the step the post says was missing.", [2, 3, 4, 5]),
 ("Rendering", "@lisyarus released a volumetric cloud path tracer that runs in the browser on WebGPU: a fast raymarched preview, a slow progressive mode by delta tracking, PNG and HDR export, and custom 3D data upload. A direct reference for the engine's cloud and fog work.", [1, 0]),
 ("Interaction nets and languages", "@zyansheep asked Fable 5.2 for reflective interaction nets and shows unary, binary and binary-tree addition running, with a viewer linked. The replies with @Aron_Adler set out the open problem: telling apart two wires that lead to identical subnets from two wires into the same subnet, inside the standard rules.", [35, 34, 33, 32, 31, 30, 29, 28, 27]),
 ("Building with agents", "@VictorTaelin argues that asking an agent to add detail to a built thing has already lost, and @kristoph answers that the fix is well-stated component boundaries.", [8, 7, 6]),
 ("Proof and the philosophy of mathematics", "A thread on Hartry Field's nominalism (physics stated without mathematical entities), a post on how to read the claim that a theorem is correct because Lean checked it, and Peter Woit on a proof whose Lean code is public while its 1,147 page explanation is held back.", [26, 25, 18, 15, 23, 24, 17, 19, 14, 13, 12, 11, 10, 9, 16, 21, 20, 22]),
]
by = {x["order"]: x for x in L}
out = ["", "## Later the same day: 36 more (read 2026-11-12, second visit)", "", "Saved in `likes-pass3.json`; media files are named `p3_<post id>`.", ""]
seen = set()
for title, note, orders in G:
    rows = [row(by[o]) for o in orders if o in by]
    seen |= set(orders)
    out += ["### %s (%d)" % (title, len(rows)), "", note, ""] + rows + [""]
assert seen == set(by), set(by) - seen
p = "C:/dev/mission-control/2026-11-12-x-likes/DIGEST.md"
s = io.open(p, encoding="utf8").read()
assert "Later the same day" not in s
io.open(p, "w", encoding="utf8", newline="\n").write(s.rstrip("\n") + "\n" + "\n".join(out))
print("appended", len(by))
