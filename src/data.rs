//! The nuclide table, and the physics you can read straight off it.

use std::sync::OnceLock;

/// IAEA ground-state data, one line per nuclide, sorted by Z then N.
const TABLE: &str = include_str!("../data/isotopes.csv");

/// How a nuclide comes apart. The IAEA spells them `B-`, `EC+B+`, `A`,
/// `SF`, `P`, `N`, `2B-`, `B-N`, and so on; these are the families worth
/// colouring the chart by.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Decay {
    Stable,
    /// Beta minus: a neutron turns into a proton. Up and left on the chart.
    BetaMinus,
    /// Electron capture or positron emission. Down and right.
    BetaPlus,
    /// Alpha: two protons and two neutrons leave together.
    Alpha,
    /// Spontaneous fission.
    Fission,
    /// Proton emission, straight off the proton drip line.
    Proton,
    /// Neutron emission, off the neutron drip line.
    Neutron,
    /// Isomeric transition and anything else that keeps Z and N.
    Other,
    /// The IAEA has no decay mode for it.
    Unknown,
}

impl Decay {
    pub fn parse(code: &str) -> Decay {
        match code {
            "" => Decay::Unknown,
            "IT" => Decay::Other,
            "SF" => Decay::Fission,
            c if c.starts_with("2B-") || c.starts_with("B-") => Decay::BetaMinus,
            c if c.starts_with("EC") || c.starts_with("B+") || c.starts_with("2EC") => {
                Decay::BetaPlus
            }
            c if c.starts_with('A') => Decay::Alpha,
            c if c.starts_with('P') || c.starts_with("2P") => Decay::Proton,
            c if c.starts_with('N') || c.starts_with("2N") => Decay::Neutron,
            _ => Decay::Other,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Decay::Stable => "stable",
            Decay::BetaMinus => "β−",
            Decay::BetaPlus => "β+ / EC",
            Decay::Alpha => "α",
            Decay::Fission => "spontaneous fission",
            Decay::Proton => "proton emission",
            Decay::Neutron => "neutron emission",
            Decay::Other => "other",
            Decay::Unknown => "unknown",
        }
    }

    /// The one- or two-character form used on arrows in a chain.
    pub fn short(&self) -> &'static str {
        match self {
            Decay::Stable => "",
            Decay::BetaMinus => "β−",
            Decay::BetaPlus => "β+",
            Decay::Alpha => "α",
            Decay::Fission => "SF",
            Decay::Proton => "p",
            Decay::Neutron => "n",
            Decay::Other => "IT",
            Decay::Unknown => "?",
        }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        match self {
            Decay::Stable => (245, 245, 245),
            Decay::BetaMinus => (90, 150, 255),
            Decay::BetaPlus => (255, 105, 90),
            Decay::Alpha => (255, 205, 70),
            Decay::Fission => (200, 120, 255),
            Decay::Proton => (255, 150, 220),
            Decay::Neutron => (110, 225, 210),
            Decay::Other => (170, 170, 175),
            Decay::Unknown => (95, 95, 105),
        }
    }

    /// Where the daughter sits, as (ΔZ, ΔN). Fission and the unknowns
    /// have no single daughter, so a chain stops there.
    pub fn step(&self) -> Option<(i32, i32)> {
        match self {
            Decay::BetaMinus => Some((1, -1)),
            Decay::BetaPlus => Some((-1, 1)),
            Decay::Alpha => Some((-2, -2)),
            Decay::Proton => Some((-1, 0)),
            Decay::Neutron => Some((0, -1)),
            _ => None,
        }
    }
}

/// One nuclide.
#[derive(Clone, Debug)]
pub struct Nuclide {
    pub z: u32,
    pub n: u32,
    pub symbol: String,
    /// Spin and parity, as the evaluators wrote it.
    pub jp: String,
    /// Natural abundance in percent, for the 288 that occur on Earth.
    pub abundance: Option<f64>,
    /// Half-life in seconds. `None` when the evaluators have no value,
    /// which is not the same as stable: 179 nuclides are known to exist
    /// without anyone having measured how long they last.
    pub half_life_s: Option<f64>,
    /// Stable as far as anybody can tell.
    pub stable: bool,
    /// Half-life as the table gives it, with its unit: "4.468E9 Y".
    pub half_life_text: String,
    /// Decay modes with branching percentages, strongest first.
    pub decays: Vec<(Decay, String, Option<f64>)>,
    /// Binding energy per nucleon, keV.
    pub binding: Option<f64>,
    /// Mass excess, keV.
    pub mass_excess: Option<f64>,
    /// Year it was first reported.
    pub discovery: Option<u32>,
}

impl Nuclide {
    pub fn a(&self) -> u32 {
        self.z + self.n
    }

    /// `U-238`.
    pub fn name(&self) -> String {
        format!("{}-{}", self.symbol, self.a())
    }

    pub fn is_stable(&self) -> bool {
        self.stable
    }

    /// The mode it mostly decays by, which is what the chart colours.
    pub fn main_decay(&self) -> Decay {
        if self.is_stable() {
            return Decay::Stable;
        }
        self.decays.first().map(|d| d.0).unwrap_or(Decay::Unknown)
    }

    /// The decay modes as a short line: at most `max` of them, and
    /// branchings below a hundredth of a percent said as "tiny" rather
    /// than printed as twelve zeroes. Uranium-234 really does emit
    /// magnesium, once in about ten trillion decays.
    pub fn decay_summary(&self, max: usize) -> String {
        if self.is_stable() {
            return "stable".into();
        }
        if self.decays.is_empty() {
            return "unknown".into();
        }
        let mut parts: Vec<String> = self
            .decays
            .iter()
            .take(max)
            .map(|(_, code, pct)| match pct {
                Some(p) if *p >= 0.01 => format!("{code} {}%", trim_zeros(*p)),
                Some(_) => format!("{code} tiny"),
                None => code.clone(),
            })
            .collect();
        if self.decays.len() > max {
            parts.push("…".into());
        }
        parts.join(", ")
    }

    /// Half-life in a unit a human reads, from nanoseconds to eons.
    pub fn half_life_pretty(&self) -> String {
        if self.stable {
            return "stable".into();
        }
        let Some(t) = self.half_life_s else { return "unknown".into() };
        const UNITS: [(f64, &str); 9] = [
            (3.156e16, "Gy"),
            (3.156e13, "My"),
            (3.156e7, "y"),
            (86400.0, "d"),
            (3600.0, "h"),
            (60.0, "min"),
            (1.0, "s"),
            (1e-3, "ms"),
            (1e-6, "µs"),
        ];
        for (scale, unit) in UNITS {
            if t >= scale {
                let v = t / scale;
                return if v >= 100.0 {
                    format!("{v:.0} {unit}")
                } else {
                    format!("{v:.3} {unit}").replace(".000 ", " ")
                };
            }
        }
        format!("{:.3} ns", t * 1e9)
    }
}

/// `99.2742` rather than `99.274200`, `100` rather than `100.0000`.
fn trim_zeros(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Every nuclide, and an index from (Z, N) into it.
pub struct Table {
    pub all: Vec<Nuclide>,
    /// `idx[z][n]` → position in `all`, for the chart and the chains.
    idx: Vec<Vec<Option<usize>>>,
    pub z_max: u32,
    pub n_max: u32,
}

impl Table {
    pub fn get(&self, z: u32, n: u32) -> Option<&Nuclide> {
        let i = *self.idx.get(z as usize)?.get(n as usize)?;
        i.map(|i| &self.all[i])
    }

    pub fn index(&self, z: u32, n: u32) -> Option<usize> {
        *self.idx.get(z as usize)?.get(n as usize)?
    }

    /// Every nuclide of one element, lightest first.
    pub fn isotopes_of(&self, z: u32) -> Vec<usize> {
        self.idx
            .get(z as usize)
            .map(|row| row.iter().flatten().copied().collect())
            .unwrap_or_default()
    }

    /// The decay chain from here down to something stable, or until the
    /// trail goes cold. Each step is (nuclide, how it got there).
    ///
    /// Fission and unknown modes end a chain: there is no single
    /// daughter to follow.
    pub fn chain(&self, start: usize) -> Vec<(usize, Decay)> {
        let mut out = vec![(start, Decay::Stable)];
        let mut cur = start;
        for _ in 0..40 {
            let nuc = &self.all[cur];
            if nuc.is_stable() {
                break;
            }
            let mode = nuc.main_decay();
            let Some((dz, dn)) = mode.step() else { break };
            let (z, n) = (nuc.z as i32 + dz, nuc.n as i32 + dn);
            if z < 0 || n < 0 {
                break;
            }
            let Some(next) = self.index(z as u32, n as u32) else { break };
            if next == cur {
                break;
            }
            out.push((next, mode));
            cur = next;
        }
        out
    }

    /// Find a nuclide by what a person would type: `U238`, `u-238`,
    /// `238U`, `Fe 56`. A bare element symbol lands on the isotope you
    /// would actually meet: the most abundant one, or failing that the
    /// longest-lived, which is what `isotopes Fe` should open on.
    pub fn find(&self, query: &str) -> Option<usize> {
        let q = query.trim().replace(['-', ' ', '_'], "").to_lowercase();
        if q.is_empty() {
            return None;
        }
        let digits: String = q.chars().filter(|c| c.is_ascii_digit()).collect();
        let letters: String = q.chars().filter(|c| c.is_ascii_alphabetic()).collect();
        if letters.is_empty() {
            return None;
        }
        if digits.is_empty() {
            return self.best_of(&letters);
        }
        let a: u32 = digits.parse().ok()?;
        self.all
            .iter()
            .position(|n| n.symbol.to_lowercase() == letters && n.a() == a)
            .or_else(|| self.best_of(&letters))
    }

    /// The isotope of an element worth opening on.
    fn best_of(&self, symbol: &str) -> Option<usize> {
        let of_element = |i: &usize| self.all[*i].symbol.to_lowercase() == symbol;
        let idx: Vec<usize> = (0..self.all.len()).filter(of_element).collect();
        idx.iter()
            .copied()
            .max_by(|&a, &b| {
                let (x, y) = (&self.all[a], &self.all[b]);
                let rank = |n: &Nuclide| {
                    (
                        n.abundance.unwrap_or(0.0),
                        if n.is_stable() { f64::INFINITY } else { n.half_life_s.unwrap_or(0.0) },
                    )
                };
                let (rx, ry) = (rank(x), rank(y));
                rx.0.total_cmp(&ry.0).then(rx.1.total_cmp(&ry.1))
            })
    }
}

pub fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| {
        let mut all: Vec<Nuclide> = Vec::with_capacity(3400);
        for line in TABLE.lines() {
            if line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.split(',').collect();
            if f.len() < 11 {
                continue;
            }
            let (Ok(z), Ok(n)) = (f[0].parse::<u32>(), f[1].parse::<u32>()) else {
                continue;
            };
            let decays = f[7]
                .split_whitespace()
                .map(|d| {
                    let (code, pct) = d.split_once(':').unwrap_or((d, ""));
                    (Decay::parse(code), code.to_string(), pct.parse().ok())
                })
                .collect();
            all.push(Nuclide {
                z,
                n,
                symbol: f[2].to_string(),
                jp: f[3].to_string(),
                abundance: f[4].parse().ok(),
                stable: f[5] == "STABLE",
                half_life_s: f[5].parse().ok(),
                half_life_text: f[6].to_string(),
                decays,
                binding: f[8].parse().ok(),
                mass_excess: f[9].parse().ok(),
                discovery: f[10].parse().ok(),
            });
        }
        let z_max = all.iter().map(|x| x.z).max().unwrap_or(0);
        let n_max = all.iter().map(|x| x.n).max().unwrap_or(0);
        let mut idx = vec![vec![None; n_max as usize + 1]; z_max as usize + 1];
        for (i, nuc) in all.iter().enumerate() {
            idx[nuc.z as usize][nuc.n as usize] = Some(i);
        }
        Table { all, idx, z_max, n_max }
    })
}

// ─────────────────────────── colour modes ────────────────────────────

pub const MODES: [&str; 5] = [
    "decay mode",
    "half-life",
    "binding energy",
    "natural abundance",
    "year discovered",
];

/// Interpolate through a list of colour stops.
fn gradient(t: f64, stops: &[(u8, u8, u8)]) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f64;
    let i = (t as usize).min(stops.len() - 2);
    let f = t - i as f64;
    let (a, b) = (stops[i], stops[i + 1]);
    let mix = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * f) as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

const HEAT: [(u8, u8, u8); 5] = [
    (60, 60, 130),
    (60, 170, 200),
    (120, 220, 120),
    (255, 210, 80),
    (255, 90, 60),
];

/// The gradient the graded colour modes run through, for legends.
pub fn heat(t: f64) -> (u8, u8, u8) {
    gradient(t, &HEAT)
}

/// The colour a nuclide gets under colour mode `mode`.
pub fn rgb_for(nuc: &Nuclide, mode: usize) -> (u8, u8, u8) {
    match mode {
        // Half-life, log scale: nanoseconds to the age of the universe
        // is 26 decades, so nothing linear would show anything.
        1 => match nuc.half_life_s {
            None => (245, 245, 245),
            Some(t) => {
                let l = t.max(1e-9).log10();
                gradient((l + 9.0) / 26.0, &HEAT)
            }
        },
        // Binding energy per nucleon: the iron peak is the point.
        2 => match nuc.binding {
            Some(b) if b > 0.0 => gradient((b - 6800.0) / (8800.0 - 6800.0), &HEAT),
            _ => (60, 60, 70),
        },
        // Natural abundance, log scale, for the 288 that have one.
        3 => match nuc.abundance {
            Some(a) if a > 0.0 => gradient((a.log10() + 4.0) / 6.0, &HEAT),
            _ => (55, 55, 65),
        },
        // Year first reported: the chart fills outward over a century.
        4 => match nuc.discovery {
            Some(y) => gradient((y as f64 - 1895.0) / 130.0, &HEAT),
            None => (55, 55, 65),
        },
        _ => nuc.main_decay().rgb(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unmeasured half-life is not the same as stable. Getting this
    /// wrong painted 179 exotic nuclides white, out on the drip lines,
    /// where nothing is stable.
    #[test]
    fn unknown_is_not_stable() {
        let t = table();
        let unknown: Vec<&Nuclide> = t
            .all
            .iter()
            .filter(|n| !n.is_stable() && n.half_life_s.is_none())
            .collect();
        assert!(unknown.len() > 150, "only {} with no half-life", unknown.len());
        assert!(unknown.iter().all(|n| n.half_life_pretty() == "unknown"));
        assert!(unknown.iter().all(|n| n.main_decay() != Decay::Stable));
        // And the genuinely stable ones stay stable.
        assert!(t.get(26, 30).unwrap().is_stable());
        assert_eq!(t.all.iter().filter(|n| n.is_stable()).count(), 244);
    }

    #[test]
    fn table_loads() {
        let t = table();
        assert!(t.all.len() > 3300, "only {}", t.all.len());
        assert_eq!(t.z_max, 118);
        assert!(t.all.iter().filter(|n| n.is_stable()).count() > 240);
    }

    #[test]
    fn uranium_238_is_right() {
        let t = table();
        let u = t.get(92, 146).unwrap();
        assert_eq!(u.name(), "U-238");
        assert_eq!(u.a(), 238);
        assert_eq!(u.main_decay(), Decay::Alpha);
        assert!((u.abundance.unwrap() - 99.2742).abs() < 0.001);
        // 4.468 billion years, in seconds.
        assert!((u.half_life_s.unwrap() - 1.41e17).abs() / 1.41e17 < 0.01);
        assert_eq!(u.discovery, Some(1896));
    }

    /// Iron-56 is the famous peak of the binding-energy curve.
    #[test]
    fn iron_is_the_most_bound() {
        let t = table();
        let best = t
            .all
            .iter()
            .filter(|n| n.binding.is_some() && n.is_stable())
            .max_by(|a, b| a.binding.unwrap().total_cmp(&b.binding.unwrap()))
            .unwrap();
        assert!(matches!(best.symbol.as_str(), "Fe" | "Ni"), "peak at {}", best.name());
        assert!((best.binding.unwrap() - 8790.0).abs() < 50.0);
    }

    /// The uranium series: U-238 alpha-decays its way to lead-206.
    #[test]
    fn uranium_series_ends_at_lead() {
        let t = table();
        let start = t.index(92, 146).unwrap();
        let chain = t.chain(start);
        let last = &t.all[chain.last().unwrap().0];
        assert_eq!(last.name(), "Pb-206", "chain ended at {}", last.name());
        assert!(chain.len() >= 14, "only {} steps", chain.len());
        // Radon-222 is in there, which is why basements get tested.
        assert!(chain.iter().any(|&(i, _)| t.all[i].name() == "Rn-222"));
    }

    /// Thorium-232 ends at lead-208, potassium-40 goes both ways but
    /// mostly beta-minus to calcium-40.
    #[test]
    fn other_chains_land_where_they_should() {
        let t = table();
        let th = t.chain(t.index(90, 142).unwrap());
        assert_eq!(t.all[th.last().unwrap().0].name(), "Pb-208");
        let k40 = t.get(19, 21).unwrap();
        assert_eq!(k40.main_decay(), Decay::BetaMinus);
    }

    #[test]
    fn search_takes_what_a_person_types() {
        let t = table();
        for q in ["U238", "u-238", "U 238"] {
            assert_eq!(t.all[t.find(q).unwrap()].name(), "U-238", "query {q}");
        }
        assert!(t.find("Fe56").is_some());
        assert!(t.find("zz9").is_none());
        // A bare symbol lands on the isotope you would actually meet.
        assert_eq!(t.all[t.find("Fe").unwrap()].name(), "Fe-56");
        assert_eq!(t.all[t.find("u").unwrap()].name(), "U-238");
        // Nothing stable, so the longest-lived one wins.
        assert_eq!(t.all[t.find("Tc").unwrap()].name(), "Tc-97");
    }

    #[test]
    fn half_lives_read_like_english() {
        let t = table();
        assert!(t.get(92, 146).unwrap().half_life_pretty().ends_with("Gy"));
        assert_eq!(t.get(26, 30).unwrap().half_life_pretty(), "stable");
        assert!(t.get(6, 8).unwrap().half_life_pretty().ends_with(" y"));
    }
}
