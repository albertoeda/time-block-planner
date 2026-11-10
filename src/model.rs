//! Core time-block model: 15-minute slots, block kinds, and a day of non-overlapping blocks.

pub const SLOTS_PER_DAY: u16 = 96;
pub const SLOT_MINUTES: u16 = 15;

/// A 15-minute boundary in a day. `Slot(0)` = 00:00, `Slot(96)` = 24:00.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot(pub u16);

impl Slot {
    pub fn label(self) -> String {
        let minutes = self.0 * SLOT_MINUTES;
        format!("{:02}:{:02}", minutes / 60, minutes % 60)
    }

    /// Parses strict `HH:MM` on the 15-minute grid, up to and including `24:00`.
    pub fn parse(s: &str) -> Option<Slot> {
        let (h, m) = s.split_once(':')?;
        if h.len() != 2 || m.len() != 2 || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (h, m): (u16, u16) = (h.parse().ok()?, m.parse().ok()?);
        if m >= 60 || m % SLOT_MINUTES != 0 {
            return None;
        }
        let slot = (h * 60 + m) / SLOT_MINUTES;
        (slot <= SLOTS_PER_DAY).then_some(Slot(slot))
    }

    fn offset(self, delta: i32) -> Option<Slot> {
        let v = self.0 as i32 + delta;
        (0..=SLOTS_PER_DAY as i32)
            .contains(&v)
            .then_some(Slot(v as u16))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Standard,
    Scheduled,
    Deep,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Standard => "standard",
            Kind::Scheduled => "scheduled",
            Kind::Deep => "deep",
        }
    }

    pub fn parse(s: &str) -> Option<Kind> {
        [Kind::Standard, Kind::Scheduled, Kind::Deep]
            .into_iter()
            .find(|k| k.as_str() == s)
    }
}

/// A time block covering the half-open range `start..end`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub start: Slot,
    pub end: Slot,
    pub kind: Kind,
    pub title: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    Overlap,
    OutOfBounds,
    Empty,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Day {
    blocks: Vec<Block>,
    /// Lines from a hand-edited file that could not be parsed; preserved verbatim.
    pub unparsed: Vec<String>,
    /// `#` comment lines from the file, written back at the top.
    pub comments: Vec<String>,
}

impl Day {
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn block_at(&self, slot: Slot) -> Option<usize> {
        self.blocks
            .iter()
            .position(|b| b.start <= slot && slot < b.end)
    }

    pub fn add(&mut self, b: Block) -> Result<usize, EditError> {
        self.check(&b, None)?;
        let idx = self.blocks.partition_point(|x| x.start < b.start);
        self.blocks.insert(idx, b);
        Ok(idx)
    }

    pub fn remove(&mut self, idx: usize) -> Block {
        self.blocks.remove(idx)
    }

    pub fn set_kind(&mut self, idx: usize, k: Kind) {
        self.blocks[idx].kind = k;
    }

    pub fn set_title(&mut self, idx: usize, t: &str) {
        self.blocks[idx].title = t.trim().replace(['\n', '\r'], " ");
    }

    /// Moves a whole block by `delta` slots. Returns its new index.
    pub fn shift(&mut self, idx: usize, delta: i32) -> Result<usize, EditError> {
        let b = &self.blocks[idx];
        let (start, end) = match (b.start.offset(delta), b.end.offset(delta)) {
            (Some(s), Some(e)) => (s, e),
            _ => return Err(EditError::OutOfBounds),
        };
        let moved = Block {
            start,
            end,
            ..b.clone()
        };
        self.check(&moved, Some(idx))?;
        self.blocks.remove(idx);
        self.add(moved)
    }

    /// Moves a block's end by `delta` slots.
    pub fn resize_end(&mut self, idx: usize, delta: i32) -> Result<(), EditError> {
        let b = &self.blocks[idx];
        let end = b.end.offset(delta).ok_or(EditError::OutOfBounds)?;
        if end <= b.start {
            return Err(EditError::Empty);
        }
        let resized = Block { end, ..b.clone() };
        self.check(&resized, Some(idx))?;
        self.blocks[idx] = resized;
        Ok(())
    }

    /// The free half-open range from `anchor` toward `other` (inclusive), clipped at neighbouring
    /// blocks. `None` when `anchor` is occupied or past the last slot.
    pub fn free_range(&self, anchor: Slot, other: Slot) -> Option<(Slot, Slot)> {
        if anchor.0 >= SLOTS_PER_DAY || self.block_at(anchor).is_some() {
            return None;
        }
        if other >= anchor {
            let next_start = self
                .blocks
                .iter()
                .map(|b| b.start)
                .filter(|s| *s > anchor)
                .min();
            let mut end = Slot(other.0.min(SLOTS_PER_DAY - 1) + 1);
            if let Some(n) = next_start {
                end = end.min(n);
            }
            Some((anchor, end))
        } else {
            let prev_end = self
                .blocks
                .iter()
                .map(|b| b.end)
                .filter(|e| *e <= anchor)
                .max();
            let start = prev_end.map_or(other, |p| other.max(p));
            Some((start, Slot(anchor.0 + 1)))
        }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.unparsed.is_empty() && self.comments.is_empty()
    }

    fn check(&self, b: &Block, ignore: Option<usize>) -> Result<(), EditError> {
        if b.end > Slot(SLOTS_PER_DAY) {
            return Err(EditError::OutOfBounds);
        }
        if b.start >= b.end {
            return Err(EditError::Empty);
        }
        let overlaps = self
            .blocks
            .iter()
            .enumerate()
            .any(|(i, x)| Some(i) != ignore && x.start < b.end && b.start < x.end);
        if overlaps {
            Err(EditError::Overlap)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blk(s: u16, e: u16) -> Block {
        Block {
            start: Slot(s),
            end: Slot(e),
            kind: Kind::Standard,
            title: String::new(),
        }
    }

    #[test]
    fn slot_label_and_parse() {
        assert_eq!(Slot(38).label(), "09:30");
        assert_eq!(Slot(96).label(), "24:00");
        assert_eq!(Slot::parse("09:30"), Some(Slot(38)));
        assert_eq!(Slot::parse("24:00"), Some(Slot(96)));
        assert_eq!(Slot::parse("09:10"), None);
        assert_eq!(Slot::parse("24:15"), None);
        assert_eq!(Slot::parse("9:3"), None);
    }

    #[test]
    fn add_rejects_overlap_and_keeps_sorted() {
        let mut d = Day::default();
        d.add(blk(40, 44)).unwrap();
        assert_eq!(d.add(blk(43, 46)), Err(EditError::Overlap));
        assert_eq!(d.add(blk(36, 40)), Ok(0));
        assert_eq!(d.block_at(Slot(43)), Some(1));
        assert_eq!(d.block_at(Slot(44)), None);
    }

    #[test]
    fn move_and_grow_refuse_out_of_bounds() {
        let mut d = Day::default();
        d.add(blk(95, 96)).unwrap();
        assert_eq!(d.shift(0, 1), Err(EditError::OutOfBounds));
        assert_eq!(d.resize_end(0, 1), Err(EditError::OutOfBounds));
        assert_eq!(d.resize_end(0, -1), Err(EditError::Empty));
        d.add(blk(0, 1)).unwrap();
        assert_eq!(d.shift(0, -1), Err(EditError::OutOfBounds));
    }

    #[test]
    fn shift_refuses_overlap() {
        let mut d = Day::default();
        d.add(blk(10, 12)).unwrap();
        d.add(blk(12, 14)).unwrap();
        assert_eq!(d.shift(0, 1), Err(EditError::Overlap));
        assert_eq!(d.blocks()[0].start, Slot(10));
    }

    #[test]
    fn free_range_clips_and_normalizes() {
        let mut d = Day::default();
        d.add(blk(20, 24)).unwrap();
        assert_eq!(d.free_range(Slot(10), Slot(30)), Some((Slot(10), Slot(20))));
        assert_eq!(d.free_range(Slot(30), Slot(10)), Some((Slot(24), Slot(31))));
        assert_eq!(d.free_range(Slot(21), Slot(30)), None);
        assert_eq!(d.free_range(Slot(5), Slot(5)), Some((Slot(5), Slot(6))));
    }

    #[test]
    fn set_title_sanitizes() {
        let mut d = Day::default();
        d.add(blk(0, 1)).unwrap();
        d.set_title(0, "  a\nb  ");
        assert_eq!(d.blocks()[0].title, "a b");
    }
}
