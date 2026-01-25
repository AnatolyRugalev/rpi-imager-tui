static TIMEZONES_DATA: &str = include_str!("../resources/timezones.txt");
static KEYBOARDS_DATA: &str = include_str!("../resources/keyboards.csv");
static LOCALES_DATA: &str = include_str!("../resources/locales.txt");

pub fn get_timezones() -> Vec<&'static str> {
    TIMEZONES_DATA.lines().filter(|l| !l.is_empty()).collect()
}

pub fn get_locales() -> Vec<&'static str> {
    LOCALES_DATA.lines().filter(|l| !l.is_empty()).collect()
}

pub fn get_keyboards() -> Vec<(&'static str, &'static str)> {
    KEYBOARDS_DATA
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(2, ',').collect();
            if parts.len() == 2 {
                Some((parts[0], parts[1]))
            } else {
                None
            }
        })
        .collect()
}
