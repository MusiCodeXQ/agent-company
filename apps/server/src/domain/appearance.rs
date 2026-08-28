use crate::domain::models::AppearanceProfile;

struct SeededRng {
    state: u64,
}

impl SeededRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223),
        }
    }

    fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    fn pick<'a>(&mut self, values: &'a [&str]) -> &'a str {
        values[(self.next() as usize) % values.len()]
    }

    fn boolean(&mut self, chance_percent: u64) -> bool {
        self.next() % 100 < chance_percent
    }
}

pub fn create_appearance(seed: u64, gender: &str, age: u8) -> AppearanceProfile {
    let mut rng = SeededRng::new(seed);
    let hair_styles = match gender {
        "female" if age >= 32 => &["bob", "wave", "side_part", "bob"][..],
        "female" => &["bob", "wave", "curly", "side_part", "wave"][..],
        "male" if age >= 32 => &["short", "side_part", "short", "side_part"][..],
        "male" => &["short", "side_part", "curly", "short"][..],
        _ => &["short", "side_part", "bob", "wave", "curly"][..],
    };
    let bottom_styles = match gender {
        "female" => &["trousers", "jeans", "skirt", "trousers"][..],
        "male" => &["trousers", "jeans", "trousers", "jeans"][..],
        _ => &["trousers", "jeans", "skirt"][..],
    };
    let beard_style = if gender == "male" && age >= 30 && rng.boolean(36) {
        Some(rng.pick(&["short", "neat", "stubble"]).to_string())
    } else {
        None
    };

    AppearanceProfile {
        seed,
        skin_tone: rng
            .pick(&["#F3C7A8", "#D99B76", "#B97852", "#8D573D", "#F0B98D"])
            .to_string(),
        hair_style: rng.pick(hair_styles).to_string(),
        hair_color: rng
            .pick(&["#241A17", "#3A2821", "#5A3928", "#1B2430", "#7B4F35"])
            .to_string(),
        top_style: rng
            .pick(&["shirt", "hoodie", "sweater", "jacket"])
            .to_string(),
        top_color: rng
            .pick(&[
                "#3E5C76", "#6B4E71", "#2E7866", "#C2704B", "#556B8A", "#9A6B3F",
            ])
            .to_string(),
        bottom_style: rng.pick(bottom_styles).to_string(),
        bottom_color: rng
            .pick(&["#26364A", "#39424E", "#5A4038", "#2C514B", "#6E5D45"])
            .to_string(),
        shoe_color: rng
            .pick(&["#1D2026", "#F2EEE8", "#A35D3B", "#334B64"])
            .to_string(),
        body_type: rng.pick(&["slim", "regular", "broad"]).to_string(),
        height_scale: 0.94 + (rng.next() % 13) as f32 / 100.0,
        glasses: rng.boolean(30),
        beard_style,
        accessory: rng
            .pick(&["watch", "headphones", "badge", "none"])
            .to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::create_appearance;

    #[test]
    fn appearance_is_stable_for_a_seed() {
        let first = create_appearance(42, "female", 31);
        let second = create_appearance(42, "female", 31);
        assert_eq!(first.seed, second.seed);
        assert_eq!(first.hair_color, second.hair_color);
        assert_eq!(first.top_color, second.top_color);
        assert_eq!(first.height_scale, second.height_scale);
    }

    #[test]
    fn gender_identity_affects_generated_character_details() {
        let male = create_appearance(42, "male", 34);
        let female = create_appearance(42, "female", 34);

        assert_ne!(male.hair_style, female.hair_style);
        assert_ne!(male.bottom_style, "skirt");
        assert!(female.beard_style.is_none());
    }
}
