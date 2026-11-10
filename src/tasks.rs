//! Nested task list: a flat vector where each item's `depth` encodes the tree.

pub const MAX_DEPTH: u8 = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub title: String,
    pub done: bool,
    pub depth: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaskList {
    items: Vec<Task>,
}

impl TaskList {
    /// Builds a list, clamping depths so the first item is 0 and each item is
    /// at most one deeper than the previous (and never above `MAX_DEPTH`).
    pub fn from_items(mut items: Vec<Task>) -> TaskList {
        let mut prev: Option<u8> = None;
        for t in &mut items {
            let cap = prev.map_or(0, |p| p + 1).min(MAX_DEPTH);
            t.depth = t.depth.min(cap);
            prev = Some(t.depth);
        }
        TaskList { items }
    }

    pub fn items(&self) -> &[Task] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Exclusive end of the subtree rooted at `i`.
    pub fn subtree_end(&self, i: usize) -> usize {
        let d = self.items[i].depth;
        self.items[i + 1..]
            .iter()
            .position(|t| t.depth <= d)
            .map_or(self.items.len(), |p| i + 1 + p)
    }

    /// Inserts an open, untitled task and returns its index.
    pub fn insert(&mut self, after: Option<usize>, child: bool) -> usize {
        let (idx, depth) = match after {
            None => (self.items.len(), 0),
            Some(a) if child => (a + 1, (self.items[a].depth + 1).min(MAX_DEPTH)),
            Some(a) => (self.subtree_end(a), self.items[a].depth),
        };
        self.items.insert(idx, Task { title: String::new(), done: false, depth });
        idx
    }

    /// Removes the subtree rooted at `i`; returns the number of items removed.
    pub fn remove(&mut self, i: usize) -> usize {
        let end = self.subtree_end(i);
        self.items.drain(i..end).count()
    }

    pub fn set_title(&mut self, i: usize, t: &str) {
        self.items[i].title = t.trim().replace(['\n', '\r'], " ");
    }

    pub fn toggle_done(&mut self, i: usize) {
        self.items[i].done = !self.items[i].done;
    }

    /// Index of the previous sibling of `i`, if any.
    fn prev_sibling(&self, i: usize) -> Option<usize> {
        let d = self.items[i].depth;
        let p = self.items[..i].iter().rposition(|t| t.depth <= d)?;
        (self.items[p].depth == d).then_some(p)
    }

    /// Makes `i` (with its subtree) a child of its previous sibling.
    pub fn indent(&mut self, i: usize) -> bool {
        if self.prev_sibling(i).is_none() {
            return false;
        }
        let end = self.subtree_end(i);
        if self.items[i..end].iter().any(|t| t.depth >= MAX_DEPTH) {
            return false;
        }
        self.items[i..end].iter_mut().for_each(|t| t.depth += 1);
        true
    }

    /// Moves `i` (with its subtree) up one level, just after its parent's
    /// subtree. Returns the new index, or `None` at depth 0.
    pub fn outdent(&mut self, i: usize) -> Option<usize> {
        let d = self.items[i].depth;
        if d == 0 {
            return None;
        }
        let parent = self.items[..i].iter().rposition(|t| t.depth < d)?;
        let parent_end = self.subtree_end(parent);
        let end = self.subtree_end(i);
        let mut sub: Vec<Task> = self.items.drain(i..end).collect();
        sub.iter_mut().for_each(|t| t.depth -= 1);
        let at = parent_end - sub.len();
        self.items.splice(at..at, sub);
        Some(at)
    }

    /// Swaps `i`'s subtree with the previous sibling's. Returns the new index.
    pub fn move_up(&mut self, i: usize) -> Option<usize> {
        let p = self.prev_sibling(i)?;
        let end = self.subtree_end(i);
        self.items[p..end].rotate_left(i - p);
        Some(p)
    }

    /// Swaps `i`'s subtree with the next sibling's. Returns the new index.
    pub fn move_down(&mut self, i: usize) -> Option<usize> {
        let end = self.subtree_end(i);
        if end >= self.items.len() || self.items[end].depth != self.items[i].depth {
            return None;
        }
        let next_end = self.subtree_end(end);
        self.items[i..next_end].rotate_left(end - i);
        Some(i + (next_end - end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(spec: &[(u8, &str)]) -> TaskList {
        TaskList::from_items(
            spec.iter()
                .map(|&(depth, title)| Task { title: title.to_string(), done: false, depth })
                .collect(),
        )
    }
    fn titles(l: &TaskList) -> Vec<(u8, &str)> {
        l.items().iter().map(|t| (t.depth, t.title.as_str())).collect()
    }

    #[test] fn from_items_normalizes_depth() {
        let l = list(&[(2, "a"), (5, "b"), (1, "c")]);
        assert_eq!(titles(&l), [(0, "a"), (1, "b"), (1, "c")]);
    }
    #[test] fn insert_sibling_goes_after_subtree() {
        let mut l = list(&[(0, "a"), (1, "a1"), (0, "b")]);
        assert_eq!(l.insert(Some(0), false), 2);
        assert_eq!(l.items()[2].depth, 0);
        assert_eq!(l.insert(Some(0), true), 1);
        assert_eq!(l.items()[1].depth, 1);
        assert_eq!(l.insert(None, false), l.len() - 1);
    }
    #[test] fn remove_takes_subtree() {
        let mut l = list(&[(0, "a"), (1, "a1"), (2, "a1x"), (0, "b")]);
        assert_eq!(l.remove(0), 3);
        assert_eq!(titles(&l), [(0, "b")]);
    }
    #[test] fn indent_needs_previous_sibling() {
        let mut l = list(&[(0, "a"), (0, "b"), (1, "b1")]);
        assert!(!l.indent(0));
        assert!(l.indent(1));
        assert_eq!(titles(&l), [(0, "a"), (1, "b"), (2, "b1")]);
        assert!(!l.indent(2)); // b1 has no previous sibling at depth 2
    }
    #[test] fn outdent_keeps_following_siblings() {
        let mut l = list(&[(0, "p"), (1, "a"), (2, "a1"), (1, "b"), (0, "q")]);
        assert_eq!(l.outdent(1), Some(2));
        assert_eq!(titles(&l), [(0, "p"), (1, "b"), (0, "a"), (1, "a1"), (0, "q")]);
        assert_eq!(l.outdent(2), None);
    }
    #[test] fn move_swaps_sibling_subtrees() {
        let mut l = list(&[(0, "a"), (1, "a1"), (0, "b")]);
        assert_eq!(l.move_down(0), Some(1));
        assert_eq!(titles(&l), [(0, "b"), (0, "a"), (1, "a1")]);
        assert_eq!(l.move_up(1), Some(0));
        assert_eq!(l.move_up(0), None);
        assert_eq!(l.move_down(2), None); // list is a, a1, b again; b is the last sibling
    }
    #[test] fn toggle_and_title() {
        let mut l = list(&[(0, "a")]);
        l.toggle_done(0); assert!(l.items()[0].done);
        l.set_title(0, " x\ny "); assert_eq!(l.items()[0].title, "x y");
    }
}
