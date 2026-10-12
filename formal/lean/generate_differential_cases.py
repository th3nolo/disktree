"""Generate the bounded corpus independently of Rust's review traversal."""

import argparse
from pathlib import Path

SEEDS = range(16)
OUTPUT = Path(__file__).with_name("differential-cases.txt")


def tree(seed: int) -> list[tuple[int | None, str]]:
    """Include empty, deep and wide trees, then seeded mixed trees."""
    count = {0: 1, 1: 10, 2: 15}.get(seed, 8 + seed * 7 % 17)
    nodes: list[tuple[int | None, str]] = [(None, "D")]
    directories = [0]
    state = seed ^ 0x9E3779B9
    for index in range(1, count):
        state = (1664525 * state + 1013904223) & 0xFFFFFFFF
        if seed == 1:
            parent, kind = index - 1, "F" if index == count - 1 else "D"
        elif seed == 2:
            parent, kind = 0, "D" if index % 3 == 0 else "F"
        else:
            parent = directories[state % len(directories)]
            kind = "D" if state >> 16 & 3 == 0 else "F"
        nodes.append((parent, kind))
        if kind == "D":
            directories.append(index)
    return nodes


def deletion_order(nodes: list[tuple[int | None, str]]) -> list[int]:
    """Independent adjacency traversal; never read a native reviewed list."""
    children = [[] for _ in nodes]
    for index, (parent, _) in enumerate(nodes[1:], 1):
        assert parent is not None and parent < index
        children[parent].append(index)
    preorder: list[int] = []

    def visit(index: int) -> None:
        preorder.append(index)
        for child in sorted(children[index]):
            visit(child)

    visit(0)
    return list(reversed(preorder))


def csv(values: list[int]) -> str:
    return ",".join(map(str, values))


def generate() -> str:
    rows = ["# DiskTree differential corpus v1; fixed seeds 0..15.",
            "# T|seed|parent,kind;...|reverse-preorder IDs",
            "# C|id|seed|mode|stop-after|removed IDs|remaining IDs|late parent"]
    cases = 0
    for seed in SEEDS:
        nodes = tree(seed)
        order = deletion_order(nodes)
        encoded = ";".join(f"{p if p is not None else '-'},{k}" for p, k in nodes)
        rows.append(f"T|{seed}|{encoded}|{csv(order)}")
        schedules = [("complete", len(order), None)]
        schedules += [("cancel", stop, None) for stop in range(len(order))]
        # The successful native callback can create an unapproved child only
        # after at least one deletion. The next directory then really fails.
        schedules += [("failure", stop, node) for stop, node in enumerate(order)
                      if stop > 0 and nodes[node][1] == "D"]
        for mode, stop, late in schedules:
            removed = order[:stop]
            remaining = [i for i in range(len(nodes)) if i not in removed]
            identifier = f"s{seed:02}-{mode}-{stop:02}"
            rows.append(f"C|{identifier}|{seed}|{mode}|{stop}|{csv(removed)}|"
                        f"{csv(remaining)}|{late if late is not None else '-'}")
            cases += 1
    rows.append(f"# {len(SEEDS)} trees; {cases} filesystem scenarios.")
    return "\n".join(rows) + "\n"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true",
                        help="refuse any corpus drift without rewriting it")
    args = parser.parse_args()
    expected = generate()
    if args.check:
        if OUTPUT.read_bytes() != expected.encode("utf-8"):
            raise SystemExit("differential corpus differs from its fixed generator")
    else:
        OUTPUT.write_text(expected, encoding="utf-8", newline="\n")
    print(expected.splitlines()[-1][2:])
