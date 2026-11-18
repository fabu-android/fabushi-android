use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

const CONCAP_CSV: &str = include_str!("../assets/concap.csv");

/// A global Dharma transmission country node combining `concap.csv`
/// coordinates with canonical Chinese country names from `COUNTRY_NAMES`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CountryNode {
    pub code: String,
    pub name_zh: String,
    pub name_en: String,
    pub capital: String,
    pub continent: String,
    pub latitude: f64,
    pub longitude: f64,
    pub server_count: usize,
}

/// Complete 249-country Chinese name mapping ported verbatim from `country_servers.dart`.
pub const COUNTRY_NAMES: &[(&str, &str)] = &[
    ("AF", "阿富汗"),
    ("AX", "奥兰群岛"),
    ("AL", "阿尔巴尼亚"),
    ("DZ", "阿尔及利亚"),
    ("AS", "美属萨摩亚"),
    ("AD", "安道尔"),
    ("AO", "安哥拉"),
    ("AI", "安圭拉"),
    ("AQ", "南极洲"),
    ("AG", "安提瓜和巴布达"),
    ("AR", "阿根廷"),
    ("AM", "亚美尼亚"),
    ("AW", "阿鲁巴"),
    ("AU", "澳大利亚"),
    ("AT", "奥地利"),
    ("AZ", "阿塞拜疆"),
    ("BS", "巴哈马"),
    ("BH", "巴林"),
    ("BD", "孟加拉国"),
    ("BB", "巴巴多斯"),
    ("BY", "白俄罗斯"),
    ("BE", "比利时"),
    ("BZ", "伯利兹"),
    ("BJ", "贝宁"),
    ("BM", "百慕大"),
    ("BT", "不丹"),
    ("BO", "玻利维亚"),
    ("BQ", "博内尔、圣尤斯特歇斯和萨巴"),
    ("BA", "波斯尼亚和黑塞哥维那"),
    ("BW", "博茨瓦纳"),
    ("BV", "布维岛"),
    ("BR", "巴西"),
    ("IO", "英属印度洋领地"),
    ("BN", "文莱达鲁萨兰国"),
    ("BG", "保加利亚"),
    ("BF", "布基纳法索"),
    ("BI", "布隆迪"),
    ("CV", "佛得角"),
    ("KH", "柬埔寨"),
    ("CM", "喀麦隆"),
    ("CA", "加拿大"),
    ("KY", "开曼群岛"),
    ("CF", "中非共和国"),
    ("TD", "乍得"),
    ("CL", "智利"),
    ("CN", "中国"),
    ("CX", "圣诞岛"),
    ("CC", "科科斯（基林）群岛"),
    ("CO", "哥伦比亚"),
    ("KM", "科摩罗"),
    ("CD", "刚果民主共和国"),
    ("CG", "刚果"),
    ("CK", "库克群岛"),
    ("CR", "哥斯达黎加"),
    ("CI", "科特迪瓦"),
    ("HR", "克罗地亚"),
    ("CU", "古巴"),
    ("CW", "库拉索"),
    ("CY", "塞浦路斯"),
    ("CZ", "捷克共和国"),
    ("DK", "丹麦"),
    ("DJ", "吉布提"),
    ("DM", "多米尼克"),
    ("DO", "多米尼加共和国"),
    ("EC", "厄瓜多尔"),
    ("EG", "埃及"),
    ("SV", "萨尔瓦多"),
    ("GQ", "赤道几内亚"),
    ("ER", "厄立特里亚"),
    ("EE", "爱沙尼亚"),
    ("SZ", "斯威士兰"),
    ("ET", "埃塞俄比亚"),
    ("FK", "福克兰群岛（马尔维纳斯）"),
    ("FO", "法罗群岛"),
    ("FJ", "斐济"),
    ("FI", "芬兰"),
    ("FR", "法国"),
    ("GF", "法属圭亚那"),
    ("PF", "法属波利尼西亚"),
    ("TF", "法属南部领地"),
    ("GA", "加蓬"),
    ("GM", "冈比亚"),
    ("GE", "格鲁吉亚"),
    ("DE", "德国"),
    ("GH", "加纳"),
    ("GI", "直布罗陀"),
    ("GR", "希腊"),
    ("GL", "格陵兰"),
    ("GD", "格林纳达"),
    ("GP", "瓜德罗普"),
    ("GU", "关岛"),
    ("GT", "危地马拉"),
    ("GG", "根西"),
    ("GN", "几内亚"),
    ("GW", "几内亚比绍"),
    ("GY", "圭亚那"),
    ("HT", "海地"),
    ("HM", "赫德岛和麦克唐纳群岛"),
    ("VA", "梵蒂冈"),
    ("HN", "洪都拉斯"),
    ("HK", "香港"),
    ("HU", "匈牙利"),
    ("IS", "冰岛"),
    ("IN", "印度"),
    ("ID", "印度尼西亚"),
    ("IR", "伊朗"),
    ("IQ", "伊拉克"),
    ("IE", "爱尔兰"),
    ("IM", "马恩岛"),
    ("IL", "以色列"),
    ("IT", "意大利"),
    ("JM", "牙买加"),
    ("JP", "日本"),
    ("JE", "泽西"),
    ("JO", "约旦"),
    ("KZ", "哈萨克斯坦"),
    ("KE", "肯尼亚"),
    ("KI", "基里巴斯"),
    ("KP", "朝鲜"),
    ("KR", "韩国"),
    ("KW", "科威特"),
    ("KG", "吉尔吉斯斯坦"),
    ("LA", "老挝人民民主共和国"),
    ("LV", "拉脱维亚"),
    ("LB", "黎巴嫩"),
    ("LS", "莱索托"),
    ("LR", "利比里亚"),
    ("LY", "利比亚"),
    ("LI", "列支敦士登"),
    ("LT", "立陶宛"),
    ("LU", "卢森堡"),
    ("MO", "澳门"),
    ("MG", "马达加斯加"),
    ("MW", "马拉维"),
    ("MY", "马来西亚"),
    ("MV", "马尔代夫"),
    ("ML", "马里"),
    ("MT", "马耳他"),
    ("MH", "马绍尔群岛"),
    ("MQ", "马提尼克"),
    ("MR", "毛里塔尼亚"),
    ("MU", "毛里求斯"),
    ("YT", "马约特"),
    ("MX", "墨西哥"),
    ("FM", "密克罗尼西亚联邦"),
    ("MD", "摩尔多瓦"),
    ("MC", "摩纳哥"),
    ("MN", "蒙古"),
    ("ME", "黑山"),
    ("MS", "蒙特塞拉特"),
    ("MA", "摩洛哥"),
    ("MZ", "莫桑比克"),
    ("MM", "缅甸"),
    ("NA", "纳米比亚"),
    ("NR", "瑙鲁"),
    ("NP", "尼泊尔"),
    ("NL", "荷兰"),
    ("NC", "新喀里多尼亚"),
    ("NZ", "新西兰"),
    ("NI", "尼加拉瓜"),
    ("NE", "尼日尔"),
    ("NG", "尼日利亚"),
    ("NU", "纽埃"),
    ("NF", "诺福克岛"),
    ("MK", "北马其顿"),
    ("MP", "北马里亚纳群岛"),
    ("NO", "挪威"),
    ("OM", "阿曼"),
    ("PK", "巴基斯坦"),
    ("PW", "帕劳"),
    ("PS", "巴勒斯坦"),
    ("PA", "巴拿马"),
    ("PG", "巴布亚新几内亚"),
    ("PY", "巴拉圭"),
    ("PE", "秘鲁"),
    ("PH", "菲律宾"),
    ("PN", "皮特凯恩"),
    ("PL", "波兰"),
    ("PT", "葡萄牙"),
    ("PR", "波多黎各"),
    ("QA", "卡塔尔"),
    ("RE", "留尼汪"),
    ("RO", "罗马尼亚"),
    ("RU", "俄罗斯"),
    ("RW", "卢旺达"),
    ("BL", "圣巴泰勒米"),
    ("SH", "圣赫勒拿、阿森松和特里斯坦-达库尼亚"),
    ("KN", "圣基茨和尼维斯"),
    ("LC", "圣卢西亚"),
    ("MF", "法属圣马丁"),
    ("PM", "圣皮埃尔和密克隆"),
    ("VC", "圣文森特和格林纳丁斯"),
    ("WS", "萨摩亚"),
    ("SM", "圣马力诺"),
    ("ST", "圣多美和普林西比"),
    ("SA", "沙特阿拉伯"),
    ("SN", "塞内加尔"),
    ("RS", "塞尔维亚"),
    ("SC", "塞舌尔"),
    ("SL", "塞拉利昂"),
    ("SG", "新加坡"),
    ("SX", "荷属圣马丁"),
    ("SK", "斯洛伐克"),
    ("SI", "斯洛文尼亚"),
    ("SB", "所罗门群岛"),
    ("SO", "索马里"),
    ("ZA", "南非"),
    ("GS", "南乔治亚和南桑威奇群岛"),
    ("SS", "南苏丹"),
    ("ES", "西班牙"),
    ("LK", "斯里兰卡"),
    ("SD", "苏丹"),
    ("SR", "苏里南"),
    ("SJ", "斯瓦尔巴和扬马延"),
    ("SE", "瑞典"),
    ("CH", "瑞士"),
    ("SY", "叙利亚"),
    ("TW", "台湾"),
    ("TJ", "塔吉克斯坦"),
    ("TZ", "坦桑尼亚"),
    ("TH", "泰国"),
    ("TL", "东帝汶"),
    ("TG", "多哥"),
    ("TK", "托克劳"),
    ("TO", "汤加"),
    ("TT", "特立尼达和多巴哥"),
    ("TN", "突尼斯"),
    ("TR", "土耳其"),
    ("TM", "土库曼斯坦"),
    ("TC", "特克斯和凯科斯群岛"),
    ("TV", "图瓦卢"),
    ("UG", "乌干达"),
    ("UA", "乌克兰"),
    ("AE", "阿拉伯联合酋长国"),
    ("GB", "英国"),
    ("UM", "美国本土外小岛屿"),
    ("US", "美国"),
    ("UY", "乌拉圭"),
    ("UZ", "乌兹别克斯坦"),
    ("VU", "瓦努阿图"),
    ("VE", "委内瑞拉"),
    ("VN", "越南"),
    ("VG", "英属维尔京群岛"),
    ("VI", "美属维尔京群岛"),
    ("WF", "瓦利斯和富图纳"),
    ("EH", "西撒哈拉"),
    ("YE", "也门"),
    ("ZM", "赞比亚"),
    ("ZW", "津巴布韦"),
];

fn fallback_geo(code: &str) -> Option<(&'static str, &'static str, f64, f64, &'static str)> {
    match code {
        "AQ" => Some(("Antarctica", "McMurdo", -75.250973, -0.071389, "Antarctica")),
        "BQ" => Some(("Caribbean Netherlands", "Kralendijk", 12.1784, -68.2385, "North America")),
        "BV" => Some(("Bouvet Island", "Bouvet", -54.4208, 3.3464, "Antarctica")),
        "GF" => Some(("French Guiana", "Cayenne", 4.9224, -52.3135, "South America")),
        "GP" => Some(("Guadeloupe", "Basse-Terre", 16.2650, -61.5510, "North America")),
        "HK" => Some(("Hong Kong", "Hong Kong", 22.3193, 114.1694, "Asia")),
        "HM" => Some((
            "Heard Island and McDonald Islands",
            "Atlas Cove",
            -53.0818,
            73.5042,
            "Antarctica",
        )),
        "MO" => Some(("Macau", "Macau", 22.1987, 113.5439, "Asia")),
        "MQ" => Some(("Martinique", "Fort-de-France", 14.6415, -61.0242, "North America")),
        "RE" => Some(("Reunion", "Saint-Denis", -21.1151, 55.5364, "Africa")),
        "YT" => Some(("Mayotte", "Mamoudzou", -12.8275, 45.1662, "Africa")),
        _ => None,
    }
}

fn server_count_for_code(code: &str) -> usize {
    match code {
        "AR" | "AU" | "BR" | "CA" | "CN" | "DE" | "DK" | "ES" | "FR" | "GB" | "IN" | "IT"
        | "KR" | "MX" | "NL" | "RU" | "US" => 3,
        _ => 2,
    }
}

#[derive(Clone)]
struct CsvRow {
    name_en: String,
    capital: String,
    lat: f64,
    lng: f64,
    continent: String,
}

static COUNTRIES_CACHE: OnceLock<Vec<CountryNode>> = OnceLock::new();

fn build_countries() -> Vec<CountryNode> {
    let mut csv_map: HashMap<String, CsvRow> = HashMap::new();
    for line in CONCAP_CSV.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<&str> = trimmed.split(',').collect();
        if parts.len() < 6 {
            continue;
        }
        let code = parts[4].trim().to_ascii_uppercase();
        if code.is_empty() || code == "NULL" {
            continue;
        }
        let lat = parts[2].trim().parse::<f64>().unwrap_or(0.0);
        let lng = parts[3].trim().parse::<f64>().unwrap_or(0.0);
        csv_map.insert(
            code,
            CsvRow {
                name_en: parts[0].trim().to_string(),
                capital: parts[1].trim().to_string(),
                lat,
                lng,
                continent: parts[5].trim().to_string(),
            },
        );
    }

    let mut nodes: Vec<CountryNode> = Vec::with_capacity(COUNTRY_NAMES.len() + 2);
    let mut seen_codes = std::collections::HashSet::new();

    for &(code, name_zh) in COUNTRY_NAMES {
        seen_codes.insert(code.to_string());
        let csv_entry = csv_map.get(code);
        let fallback = fallback_geo(code);

        let (name_en, capital, lat, lng, continent) = match (csv_entry, fallback) {
            (Some(row), Some(fb)) if row.lat == 0.0 && row.lng == 0.0 => (
                row.name_en.clone(),
                if row.capital == "N/A" {
                    fb.1.to_string()
                } else {
                    row.capital.clone()
                },
                fb.2,
                fb.3,
                row.continent.clone(),
            ),
            (Some(row), _) => (
                row.name_en.clone(),
                row.capital.clone(),
                row.lat,
                row.lng,
                row.continent.clone(),
            ),
            (None, Some(fb)) => (
                fb.0.to_string(),
                fb.1.to_string(),
                fb.2,
                fb.3,
                fb.4.to_string(),
            ),
            (None, None) => (
                code.to_string(),
                String::new(),
                0.0,
                0.0,
                String::new(),
            ),
        };

        nodes.push(CountryNode {
            code: code.to_string(),
            name_zh: name_zh.to_string(),
            name_en,
            capital,
            continent,
            latitude: lat,
            longitude: lng,
            server_count: server_count_for_code(code),
        });
    }

    if let Some(ko) = csv_map.get("KO") {
        if !seen_codes.contains("KO") {
            nodes.push(CountryNode {
                code: "KO".to_string(),
                name_zh: "科索沃".to_string(),
                name_en: ko.name_en.clone(),
                capital: ko.capital.clone(),
                continent: ko.continent.clone(),
                latitude: ko.lat,
                longitude: ko.lng,
                server_count: 2,
            });
        }
    }

    nodes.sort_by(|a, b| a.name_zh.cmp(&b.name_zh).then_with(|| a.code.cmp(&b.code)));
    nodes
}

/// Returns all 249+ global country nodes parsed from `concap.csv` and `COUNTRY_NAMES`.
pub fn all_countries() -> &'static [CountryNode] {
    COUNTRIES_CACHE.get_or_init(build_countries).as_slice()
}

/// Looks up a country node by two-letter ISO code (case-insensitive).
pub fn country_by_code(code: &str) -> Option<&'static CountryNode> {
    let target = code.trim().to_ascii_uppercase();
    all_countries().iter().find(|c| c.code == target)
}

/// Finds the first country node within `tolerance_deg` of the given coordinates.
pub fn country_by_coordinates(lat: f64, lng: f64, tolerance_deg: f64) -> Option<&'static CountryNode> {
    all_countries()
        .iter()
        .find(|c| (c.latitude - lat).abs() < tolerance_deg && (c.longitude - lng).abs() < tolerance_deg)
}

/// Searches countries by ISO code, Chinese name, English name, capital, or continent.
pub fn search_countries(query: &str) -> Vec<&'static CountryNode> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return all_countries().iter().collect();
    }
    all_countries()
        .iter()
        .filter(|c| {
            c.code.to_ascii_lowercase().contains(&q)
                || c.name_zh.to_ascii_lowercase().contains(&q)
                || c.name_en.to_ascii_lowercase().contains(&q)
                || c.capital.to_ascii_lowercase().contains(&q)
                || c.continent.to_ascii_lowercase().contains(&q)
        })
        .collect()
}

/// Resolves a country code to its Chinese display name, falling back to the code itself.
pub fn country_name_zh(code: &str) -> &str {
    country_by_code(code)
        .map(|c| c.name_zh.as_str())
        .unwrap_or(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_at_least_249_countries_with_chinese_names() {
        let countries = all_countries();
        assert!(countries.len() >= 249, "expected >= 249 countries, got {}", countries.len());
        let cn = country_by_code("cn").expect("CN must exist");
        assert_eq!(cn.name_zh, "中国");
        assert_eq!(cn.capital, "Beijing");
        assert!((cn.latitude - 39.9166).abs() < 0.05);
        assert!((cn.longitude - 116.3833).abs() < 0.05);

        let hk = country_by_code("HK").expect("HK must exist");
        assert_eq!(hk.name_zh, "香港");
        assert!(hk.latitude.abs() > 1.0 && hk.longitude.abs() > 1.0);
    }

    #[test]
    fn search_countries_matches_code_chinese_and_english() {
        let by_code = search_countries("jp");
        assert!(by_code.iter().any(|c| c.code == "JP" && c.name_zh == "日本"));
        let by_zh = search_countries("印度");
        assert!(by_zh.iter().any(|c| c.code == "IN"));
        let by_en = search_countries("united kingdom");
        assert!(by_en.iter().any(|c| c.code == "GB"));
        assert_eq!(search_countries("").len(), all_countries().len());
    }
}
