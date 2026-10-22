//! Patch library: preset banks (Hypersonic 2 content folder or the built-in
//! factory set) and user banks, with the Load page search.

use crate::bank::{self, BankKind};
use crate::patch::Patch;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Bank {
    /// Bank number (preset banks 0.., user banks 100..).
    pub number: u32,
    pub name: String,
    pub user: bool,
    pub patches: Vec<Arc<Patch>>,
}

#[derive(Clone, Debug, Default)]
pub struct Library {
    pub banks: Vec<Bank>,
    /// Sample bank names (number, bank name, sample names) for the Sample selector.
    pub sample_banks: Vec<(u32, String, Vec<String>)>,
    pub phrase_banks: Vec<(u32, String, Vec<String>)>,
    /// Phrases for the arpeggiator, by phrase bank number.
    pub phrases: Vec<(u32, Vec<Arc<crate::phrase::Phrase>>)>,
    pub tuning_banks: Vec<(u32, String)>,
    /// Combis (.fxb) of the content folder.
    pub combis: Vec<Arc<crate::combi::Combi>>,
}

impl Library {
    /// Add a content file by its file name ("016 Acoustic Basses") and data.
    /// Returns what kind of bank it was.
    pub fn add_file(&mut self, file_name: &str, data: &[u8]) -> Option<BankKind> {
        let (kind, _, num) = bank::header(data)?;
        let name = bank::split_file_name(file_name).map(|(_, n)| n.to_string()).unwrap_or_else(|| file_name.to_string());
        match kind {
            BankKind::Patch => {
                let patches: Vec<Arc<Patch>> = bank::patches(data).into_iter().map(Arc::new).collect();
                if patches.is_empty() {
                    return None;
                }
                self.banks.retain(|b| !(b.number == num && !b.user));
                self.banks.push(Bank { number: num, name, user: false, patches });
                self.banks.sort_by_key(|b| (b.number, b.user));
            }
            BankKind::Samples => {
                self.sample_banks.retain(|b| b.0 != num);
                self.sample_banks.push((num, name, bank::names(data)));
                self.sample_banks.sort_by_key(|b| b.0);
            }
            BankKind::Phrases => {
                let ph: Vec<Arc<crate::phrase::Phrase>> = crate::phrase::parse(data).into_iter().map(Arc::new).collect();
                self.phrase_banks.retain(|b| b.0 != num);
                self.phrase_banks.push((num, name, ph.iter().map(|p| p.name.clone()).collect()));
                self.phrase_banks.sort_by_key(|b| b.0);
                self.phrases.retain(|b| b.0 != num);
                self.phrases.push((num, ph));
                self.phrases.sort_by_key(|b| b.0);
            }
            BankKind::Tunings => {
                self.tuning_banks.retain(|b| b.0 != num);
                self.tuning_banks.push((num, name));
            }
            _ => {}
        }
        Some(kind)
    }

    /// Add a combi file (.fxb); returns false when it is not a combi.
    pub fn add_combi(&mut self, data: &[u8]) -> bool {
        match crate::combi::parse(data) {
            Some(c) if !c.parts.is_empty() => {
                self.combis.push(Arc::new(c));
                true
            }
            _ => false,
        }
    }

    /// Add (or replace) a user bank of patches.
    pub fn add_user_bank(&mut self, name: &str, patches: Vec<Patch>) -> u32 {
        let number = self.banks.iter().filter(|b| b.user).map(|b| b.number + 1).max().unwrap_or(100);
        self.banks.push(Bank { number, name: name.into(), user: true, patches: patches.into_iter().map(Arc::new).collect() });
        number
    }

    pub fn bank(&self, number: u32) -> Option<&Bank> {
        self.banks.iter().find(|b| b.number == number)
    }

    pub fn bank_mut(&mut self, number: u32) -> Option<&mut Bank> {
        self.banks.iter_mut().find(|b| b.number == number)
    }

    /// MIDI bank select + program change.
    pub fn program(&self, bank: u32, prog: u32) -> Option<Arc<Patch>> {
        self.bank(bank)?.patches.get(prog as usize).cloned()
    }

    /// Load page search: (bank number, patch index).
    pub fn search(&self, query: &str) -> Vec<(u32, usize)> {
        let mut out = Vec::new();
        if query.trim().is_empty() {
            return out;
        }
        for b in &self.banks {
            for (i, p) in b.patches.iter().enumerate() {
                if p.matches(query) {
                    out.push((b.number, i));
                }
            }
        }
        out
    }

    /// Next / previous patch across the whole library ([Shift]+arrow).
    pub fn step(&self, bank: u32, idx: usize, dir: i32) -> Option<(u32, usize)> {
        let flat: Vec<(u32, usize)> = self.banks.iter().flat_map(|b| (0..b.patches.len()).map(move |i| (b.number, i))).collect();
        let pos = flat.iter().position(|&x| x == (bank, idx))?;
        let n = flat.len() as i32;
        Some(flat[((pos as i32 + dir).rem_euclid(n)) as usize])
    }

    pub fn sample_name(&self, bank: u32, wave: u32) -> String {
        match self.sample_banks.iter().find(|b| b.0 == bank) {
            Some((_, name, names)) => match names.get(wave as usize) {
                Some(n) => format!("{name}: {n}"),
                None => name.clone(),
            },
            None => format!("Bank {bank:03} / {wave}"),
        }
    }

    pub fn total_patches(&self) -> usize {
        self.banks.iter().map(|b| b.patches.len()).sum()
    }
}

/// Keyword menu of the Load page (categories and attributes).
pub const KEYWORDS: [&str; 40] = [
    "Piano", "E-Piano", "Organ", "Clavinet", "Harpsichord", "Mallet", "Bell", "Bass", "Guitar", "Strings", "Orchestra",
    "Brass", "Sax", "Woodwind", "Choir", "Vocal", "Pad", "Lead", "Synth", "Arp", "Drums", "Percussion", "Loop", "FX",
    "Ethnic", "Warm", "Bright", "Soft", "Hard", "Fat", "Thin", "Rich", "Dark", "Mellow", "Metallic", "Breathy",
    "Pulsing", "Reverse", "Natural", "Resonant",
];
