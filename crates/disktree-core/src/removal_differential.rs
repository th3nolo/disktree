//! Seeded model/native comparisons create only Fixture-owned temporary trees.

use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const CORPUS: &str =
    include_str!("../../../formal/lean/differential-cases.txt");
const HEADER: &str = "case|removed|remaining|late|canaries\n";
const CASE_COUNT: usize = 322;

#[derive(Debug)]
struct Node {
    parent: Option<usize>,
    directory: bool,
}

#[derive(Debug)]
struct Tree {
    nodes: Vec<Node>,
    order: Vec<usize>,
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Complete,
    Cancel,
    Failure,
}

#[derive(Debug)]
struct Case<'a> {
    id: &'a str,
    seed: usize,
    mode: Mode,
    stop: usize,
    removed: Vec<usize>,
    remaining: Vec<usize>,
    late: Option<usize>,
}

fn ids(text: &str) -> Vec<usize> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split(',')
            .map(|id| id.parse().expect("numeric fixture ID"))
            .collect()
    }
}

fn csv(values: &[usize]) -> String {
    values
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

impl Tree {
    fn parse(nodes: &str, order: &str) -> Self {
        let mut parsed: Vec<Node> = Vec::new();
        for (id, node) in nodes.split(';').enumerate() {
            let (parent, kind) = node.split_once(',').expect("node");
            let parent = if parent == "-" {
                None
            } else {
                Some(parent.parse::<usize>().expect("parent"))
            };
            assert!(matches!(kind, "D" | "F"));
            if id == 0 {
                assert!(parent.is_none() && kind == "D");
            } else {
                let parent = parent.expect("child parent");
                assert!(parent < id && parsed[parent].directory);
            }
            parsed.push(Node {
                parent,
                directory: kind == "D",
            });
        }
        assert!(!parsed.is_empty() && parsed.len() <= 32);
        let order = ids(order);
        assert_eq!(order.len(), parsed.len());
        assert_eq!(
            order.iter().copied().collect::<BTreeSet<_>>(),
            (0..parsed.len()).collect()
        );
        Self {
            nodes: parsed,
            order,
        }
    }
}

impl<'a> Case<'a> {
    fn parse(fields: &[&'a str]) -> Self {
        assert_eq!(fields.len(), 8);
        Self {
            id: fields[1],
            seed: fields[2].parse().expect("seed"),
            mode: match fields[3] {
                "complete" => Mode::Complete,
                "cancel" => Mode::Cancel,
                "failure" => Mode::Failure,
                _ => panic!("unknown scenario"),
            },
            stop: fields[4].parse().expect("boundary"),
            removed: ids(fields[5]),
            remaining: ids(fields[6]),
            late: (fields[7] != "-")
                .then(|| fields[7].parse().expect("late parent")),
        }
    }

    fn validate(&self, tree: &Tree) {
        assert!(self.stop <= tree.nodes.len());
        assert_eq!(self.removed, tree.order[..self.stop]);
        assert_eq!(
            self.remaining,
            (0..tree.nodes.len())
                .filter(|id| !self.removed.contains(id))
                .collect::<Vec<_>>()
        );
        match self.mode {
            Mode::Complete => {
                assert_eq!(self.stop, tree.nodes.len());
                assert!(self.late.is_none());
            }
            Mode::Cancel => {
                assert!(self.stop < tree.nodes.len() && self.late.is_none());
            }
            Mode::Failure => {
                let parent = self.late.expect("late child failure");
                assert!(self.stop > 0 && self.stop < tree.nodes.len());
                assert_eq!(tree.order[self.stop], parent);
                assert!(tree.nodes[parent].directory);
            }
        }
    }
}

struct NativeTree {
    fixture: Fixture,
    paths: Vec<PathBuf>,
    identities: Vec<EntryIdentity>,
    contents: Vec<Option<Vec<u8>>>,
    control_ids: Vec<EntryIdentity>,
}

impl NativeTree {
    fn new(seed: usize, tree: &Tree) -> Self {
        let fixture = Fixture::new();
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut identities = Vec::new();
        let mut contents = Vec::new();
        for (id, node) in tree.nodes.iter().enumerate() {
            let path = node.parent.map_or_else(
                || fixture.root.join("marked"),
                |parent| {
                    paths[parent].join(format!(
                        "n{id:04}{}",
                        if node.directory { "" } else { ".bin" }
                    ))
                },
            );
            let bytes = if node.directory {
                fs::create_dir(&path).expect("owned directory");
                None
            } else {
                let data = format!(
                    "seed={seed};node={id};{}",
                    "reviewed".repeat(1 + (seed + id) % 7)
                )
                .into_bytes();
                fs::write(&path, &data).expect("owned file");
                Some(data)
            };
            identities.push(entry_snapshot(&path).expect("identity").identity);
            paths.push(path);
            contents.push(bytes);
        }
        assert_eq!(
            identities.iter().collect::<BTreeSet<_>>().len(),
            paths.len()
        );
        let control_ids = fixture
            .canaries
            .iter()
            .map(|(path, _)| {
                entry_snapshot(path).expect("control identity").identity
            })
            .collect();
        Self {
            fixture,
            paths,
            identities,
            contents,
            control_ids,
        }
    }

    fn observe(
        &self,
        case: &Case<'_>,
        tree: &Tree,
        removed: &[usize],
    ) -> String {
        let mut remaining = Vec::new();
        for (id, path) in self.paths.iter().enumerate() {
            if removed.contains(&id) {
                assert!(!path.exists(), "deleted node survives: {}", case.id);
            } else {
                let actual = entry_snapshot(path).expect("surviving identity");
                assert_eq!(actual.identity, self.identities[id]);
                assert_eq!(actual.kind.is_dir(), tree.nodes[id].directory);
                if let Some(bytes) = &self.contents[id] {
                    assert_eq!(fs::read(path).expect("survivor bytes"), *bytes);
                } else {
                    let mut expected = tree
                        .nodes
                        .iter()
                        .enumerate()
                        .filter(|(child, node)| {
                            node.parent == Some(id) && !removed.contains(child)
                        })
                        .map(|(child, _)| self.paths[child].clone())
                        .collect::<BTreeSet<_>>();
                    if case.late == Some(id) {
                        expected.insert(path.join("late-unapproved.bin"));
                    }
                    let actual = fs::read_dir(path)
                        .expect("surviving directory")
                        .map(|entry| entry.expect("surviving child").path())
                        .collect::<BTreeSet<_>>();
                    assert_eq!(actual, expected, "unexpected namespace effect");
                }
                remaining.push(id);
            }
        }
        self.fixture.assert_canaries();
        for ((path, _), identity) in
            self.fixture.canaries.iter().zip(&self.control_ids)
        {
            assert_eq!(
                entry_snapshot(path).expect("control survives").identity,
                *identity
            );
        }
        let late_present = case.late.is_some_and(|parent| {
            let late = self.paths[parent].join("late-unapproved.bin");
            assert_eq!(
                fs::read(late).expect("late child survives"),
                b"never approved"
            );
            true
        });
        assert_eq!(remaining, case.remaining);
        assert_eq!(removed, case.removed);
        format!(
            "{}|{}|{}|{}|1000000,1000001,1000002\n",
            case.id,
            csv(removed),
            csv(&remaining),
            usize::from(late_present)
        )
    }
}

fn native_case(case: &Case<'_>, tree: &Tree) -> String {
    case.validate(tree);
    let native = NativeTree::new(case.seed, tree);
    let cancel = AtomicBool::new(false);
    let review = crate::windows::review_tree(
        &native.paths[0],
        &cancel,
        crate::windows::REVIEW_ENTRY_LIMIT,
    )
    .expect("generated review");
    assert_eq!(review.len(), tree.nodes.len());
    assert_eq!(review.identity(), Some(native.identities[0]));
    cancel.store(
        case.mode == Mode::Cancel && case.stop == 0,
        Ordering::Relaxed,
    );
    let mut removed = Vec::new();
    let result = crate::windows::remove_reviewed(
        &native.paths[0],
        &review,
        &cancel,
        |path| {
            let id = native
                .paths
                .iter()
                .position(|p| p == path)
                .expect("only a declared node can be removed");
            assert!(!removed.contains(&id), "duplicate deletion");
            for previous in &removed {
                assert!(!path.starts_with(&native.paths[*previous]));
            }
            removed.push(id);
            if removed.len() == case.stop {
                match case.mode {
                    Mode::Cancel => cancel.store(true, Ordering::Relaxed),
                    Mode::Failure => {
                        let parent = case.late.expect("failed directory");
                        fs::write(
                            native.paths[parent].join("late-unapproved.bin"),
                            b"never approved",
                        )
                        .expect("post-preflight late child");
                    }
                    Mode::Complete => {}
                }
            }
        },
    );
    match case.mode {
        Mode::Complete => result.expect("complete generated removal"),
        Mode::Cancel => assert_eq!(
            result.expect_err("cancelled").kind(),
            io::ErrorKind::Interrupted
        ),
        Mode::Failure => {
            let error = result.expect_err("real nonempty-directory failure");
            assert_eq!(error.kind(), io::ErrorKind::DirectoryNotEmpty);
            assert!(error.to_string().contains(&format!(
                "{} reviewed entries already deleted",
                case.stop
            )));
        }
    }
    native.observe(case, tree, &removed)
}

#[test]
fn generated_tree_schedules_match_lean_and_native_effects() {
    let model = std::env::var_os("DISKTREE_LEAN_RESULTS")
        .map(|path| fs::read_to_string(path).expect("actual Lean transcript"));
    if let Some(model) = &model {
        assert!(model.starts_with(HEADER));
        assert_eq!(model.lines().count(), CASE_COUNT + 1);
    }
    let mut output = std::env::var_os("DISKTREE_NATIVE_RESULTS").map(|path| {
        assert!(
            model.is_some(),
            "an exported differential run requires Lean"
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("fresh native evidence destination");
        file.write_all(HEADER.as_bytes()).expect("evidence header");
        file
    });
    let mut trees = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut count = 0;
    for line in CORPUS
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let fields = line.split('|').collect::<Vec<_>>();
        match fields[0] {
            "T" => {
                assert_eq!(fields.len(), 4);
                let seed = fields[1].parse::<usize>().expect("tree seed");
                assert!(seed < 16);
                assert!(
                    trees
                        .insert(seed, Tree::parse(fields[2], fields[3]))
                        .is_none()
                );
            }
            "C" => {
                let case = Case::parse(&fields);
                assert!(seen.insert(case.id), "duplicate scenario");
                let actual =
                    native_case(&case, trees.get(&case.seed).expect("tree"));
                if let Some(file) = &mut output {
                    file.write_all(actual.as_bytes())
                        .expect("observed effects");
                }
                if let Some(model) = &model {
                    assert_eq!(
                        actual.trim_end(),
                        model
                            .lines()
                            .nth(count + 1)
                            .expect("matching model row"),
                        "Lean/native filesystem effects differ in {}",
                        case.id
                    );
                }
                count += 1;
            }
            _ => panic!("unknown corpus record"),
        }
    }
    assert_eq!(trees.len(), 16);
    assert_eq!(count, CASE_COUNT);
    println!(
        "16 generated trees; {count} real filesystem scenarios; \
        Lean transcript compared: {}",
        model.is_some()
    );
}
