"""Summarize the complete, exactly replayed M6 width experiment."""

from collections import Counter

from m6 import atomic_json, read
from m6_campaign import lines
from m6_width import ROOT
from sailing import REPO


def checked(path):
    episodes = lines(path / "audit.jsonl")
    index = lines(path / "store/index.jsonl")
    assert episodes and [e["episode_id"] for e in episodes] == [
        e["episode_id"] for e in index
    ]
    return episodes


def counts(rows):
    return dict(
        n=len(rows),
        wins=sum(e["outcome"] == "finished" for e in rows),
        settled=sum(e["tacks"] > 0 for e in rows),
        settled_wins=sum(e["tacks"] > 0 and e["outcome"] == "finished" for e in rows),
        outcomes=dict(Counter(e["reason"] or e["outcome"] for e in rows)),
    )


def results():
    status = read(ROOT / "status.json")
    assert status["phase"] == "complete"
    protocol = read(ROOT / "protocol.json")
    ret, fits, controls = {}, {}, {}
    audits = 0
    previous = set()
    for stem in sorted(protocol["checkpoints"]):
        rows = checked(ROOT / "retention" / stem)
        audits += len(rows)
        assert [r["condition"] for r in rows] == [
            r["condition"]["id"] for r in protocol["panels"]["broad"]
        ]
        won = {r["condition"] for r in rows if r["outcome"] == "finished"}
        ret[stem] = dict(
            inside=counts([r for r in rows if abs(r["m5_start_xy"][0]) <= 5]),
            outside=counts([r for r in rows if abs(r["m5_start_xy"][0]) > 5]),
            won=sorted(won),
            gained=sorted(won - previous),
            lost=sorted(previous - won),
        )
        previous = won
    for arm in status["arms"]:
        rows = checked(ROOT / arm / "controls")
        controls[arm] = {
            k: counts([r for r in rows if r["kind"] == k]) for k in ("neutral", "pink")
        }
        audits += len(rows)
        for out in sorted((ROOT / arm / "pilot").iterdir()):
            train = checked(out)
            scores = checked(out / "evaluation")
            audits += len(train) + len(scores)
            name = f"{arm}/{out.name}"
            probe = lines(out / "probes.jsonl")
            row = read(out / "summary.json")
            row.update(
                qualifying_checkpoint=read(out / "qualifying.json")["decisions"]
                if (out / "qualifying.json").exists()
                else None,
                training=counts(
                    [
                        r
                        for r in train
                        if r["kind"] == "training" and r["outcome"] != "running"
                    ]
                ),
                promotion_probes=probe,
                panels={
                    k: counts([r for r in scores if r["kind"] == k])
                    for k in ["broad", "narrow", "heading", "original_validation"]
                },
            )
            if (out / "transfer").exists():
                rows = checked(out / "transfer")
                audits += len(rows)
                row["transfer"] = {
                    f"{k}_{b}": counts(
                        [r for r in rows if r["geometry"] == k and r["board"] == b]
                    )
                    for k in ("direct", "switch")
                    for b in (-1, 1)
                }
            fits[name] = row
    return dict(
        elapsed_s=status["updated"] - protocol["start"],
        audited_episodes=audits,
        retention=ret,
        controls=controls,
        runs=fits,
        decision=status,
    )


def report(data):
    d = REPO / "docs/ese3990"
    atomic_json(d / "M6_width_results.json", data)
    text = [
        "# M6 follow-up · Lateral offset and initial heading",
        "",
        f"The campaign took {data['elapsed_s'] / 60:.1f} minutes and exactly replayed "
        f"{data['audited_episodes']:,} episode records. Each fresh fit used 400,000 "
        "decisions, including 10,000 for calibration. A seed qualified on two "
        "successive fresh probes at 6/20 or higher, each above both controls. "
        "[Frozen plan](ESE3990_SailGym_Project_Plan.md"
        "#m6-follow-up--learn-the-first-approach-before-moving-downwind) · "
        "[Complete numerical results](M6_width_results.json)",
        "",
        "## Fixed-panel retention on the first seed-100 PPO campaign",
        "",
        "| Decisions | Inside gate span / 40 | Outside / 40 | "
        "Gained since previous | Lost since previous |",
        "|---:|---:|---:|---:|---:|",
    ]
    for i, (k, v) in enumerate(data["retention"].items()):
        text.append(
            f"| {int(k):,} | {v['inside']['wins']} | {v['outside']['wins']} | "
            f"{len(v['gained']) if i else '–'} | {len(v['lost']) if i else '–'} |"
        )
    text += [
        "",
        "The same 80 condition identities appear in all six rows. "
        "The numerical results list each gained and lost finish by condition ID.",
        "",
        "## Fixed-final panels and fresh promotion probes",
        "",
        "| Training geometry | Learner / seed | Promotion finishes per 20 | "
        "First qualifying decision | Narrow / 80 | Broad / 80 | "
        "Close-hauled / 80 | Original 40 m / 20 |",
        "|---|---|---|---:|---:|---:|---:|---:|",
    ]
    for name, row in data["runs"].items():
        arm, learner = name.split("/")
        cols = " | ".join(
            str(row["panels"][k]["wins"])
            for k in ("narrow", "broad", "heading", "original_validation")
        )
        probe_scores = ", ".join(str(r["finishes"]) for r in row["promotion_probes"])
        text.append(
            f"| {arm} | {learner} | {probe_scores} | "
            f"{row['qualifying_checkpoint'] or '–'} | {cols} |"
        )
    text += [
        "",
        "Controls used 100 starts on each geometry. Neutral and pink-noise "
        "finishes respectively were "
        + ", ".join(
            f"{a} {x['neutral']['wins']}/100 and {x['pink']['wins']}/100"
            for a, x in data["controls"].items()
        )
        + ". All fixed-final panels use deterministic policies. "
        "The random-heading narrow and broad panels share wind, heading, depth "
        "and normalized lateral offset. The close-hauled panel shares positions "
        "and winds with the narrow panel.",
        "",
    ]
    text.extend(["## Campaign decision", ""])
    for arm in data["decision"]["arms"]:
        fits = [r for n, r in data["runs"].items() if n.startswith(f"{arm}/ppo_")]
        qualified = sum(bool(r["qualifying_checkpoint"]) for r in fits)
        broad = sum(r["panels"]["broad"]["wins"] for r in fits)
        narrow = sum(r["panels"]["narrow"]["wins"] for r in fits)
        text.append(
            f"- {arm.capitalize()} training qualified {qualified}/{len(fits)} seeds. "
            f"Fixed-final policies finished {broad}/{80 * len(fits)} broad and "
            f"{narrow}/{80 * len(fits)} narrow random-heading evaluations."
        )
    text.append("")
    if len(data["decision"]["passing"]) < 2:
        text.extend(
            [
                "Both changes missed the two-seed threshold. The campaign "
                "closed before the 3–7 m transfer or SAC pilot. The common-panel "
                "comparison shows which successful approaches the first seed "
                "lost as training continued. The mechanism remains open.",
                "",
            ]
        )
    for name, row in data["runs"].items():
        if "transfer" in row:
            text.extend(
                [f"**{name} transfer** from its first qualifying snapshot.", ""]
            )
            for k, v in row["transfer"].items():
                text.append(
                    f"- {k} finished {v['wins']}/{v['n']}, including "
                    f"{v['settled_wins']} finishes with a settled tack."
                )
            text.append("")
    text.append(
        "[Run protocol and panels](../../runs/ese3990/M6/width/protocol.json) · "
        "[Per-episode audits and checkpoints](../../runs/ese3990/M6/width) · "
        "[Campaign runner](../../experiments/ese3990/m6_width.py)"
    )
    (d / "M6_width_report.md").write_text("\n".join(text) + "\n")


if __name__ == "__main__":
    report(results())
