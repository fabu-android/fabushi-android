//! 全球法布施 (Global Dharma Offering) Mini-App Domain Engine:
//! - 8 Canonical Buddhist Sutras & 12 Dharma Ritual Packages (`r2-files-list.json`)
//! - 249+ Global Country Nodes & Coordinates (`concap.csv`)
//! - Zen Room Spinning Prayer Wheel Merit & Mantra Counter
//! - Practice Sessions, Streak & 21-Badge Achievement System
//! - Global Merit Leaderboard & AI Dharma Counselor

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

const CONCAP_CSV: &str = include_str!("../assets/concap.csv");
const R2_FILES_LIST_JSON: &str = include_str!("../assets/r2-files-list.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sutra {
    pub id: String,
    pub title: String,
    pub category: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DharmaPackage {
    pub key: String,
    pub display_name: String,
    pub size_bytes: u64,
    pub uploaded: String,
}

#[derive(Deserialize)]
struct R2FilesList {
    #[serde(default)]
    objects: Vec<R2FileEntry>,
    #[serde(default)]
    files: Vec<R2FileEntry>,
}

#[derive(Deserialize)]
struct R2FileEntry {
    key: String,
    size: u64,
    #[serde(default)]
    uploaded: String,
}

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

pub const COUNTRY_NAMES: &[(&str, &str)] = &[
    ("AF", "阿富汗"), ("AX", "奥兰群岛"), ("AL", "阿尔巴尼亚"), ("DZ", "阿尔及利亚"),
    ("AS", "美属萨摩亚"), ("AD", "安道尔"), ("AO", "安哥拉"), ("AI", "安圭拉"),
    ("AQ", "南极洲"), ("AG", "安提瓜和巴布达"), ("AR", "阿根廷"), ("AM", "亚美尼亚"),
    ("AW", "阿鲁巴"), ("AU", "澳大利亚"), ("AT", "奥地利"), ("AZ", "阿塞拜疆"),
    ("BS", "巴哈马"), ("BH", "巴林"), ("BD", "孟加拉国"), ("BB", "巴巴多斯"),
    ("BY", "白俄罗斯"), ("BE", "比利时"), ("BZ", "伯利兹"), ("BJ", "贝宁"),
    ("BM", "百慕大"), ("BT", "不丹"), ("BO", "玻利维亚"), ("BQ", "博内尔、圣尤斯特歇斯和萨巴"),
    ("BA", "波斯尼亚和黑塞哥维那"), ("BW", "博茨瓦纳"), ("BV", "布维岛"), ("BR", "巴西"),
    ("IO", "英属印度洋领地"), ("BN", "文莱达鲁萨兰国"), ("BG", "保加利亚"), ("BF", "布基纳法索"),
    ("BI", "布隆迪"), ("CV", "佛得角"), ("KH", "柬埔寨"), ("CM", "喀麦隆"),
    ("CA", "加拿大"), ("KY", "开曼群岛"), ("CF", "中非共和国"), ("TD", "乍得"),
    ("CL", "智利"), ("CN", "中国"), ("CX", "圣诞岛"), ("CC", "科科斯（基林）群岛"),
    ("CO", "哥伦比亚"), ("KM", "科摩罗"), ("CD", "刚果民主共和国"), ("CG", "刚果"),
    ("CK", "库克群岛"), ("CR", "哥斯达黎加"), ("CI", "科特迪瓦"), ("HR", "克罗地亚"),
    ("CU", "古巴"), ("CW", "库拉索"), ("CY", "塞浦路斯"), ("CZ", "捷克共和国"),
    ("DK", "丹麦"), ("DJ", "吉布提"), ("DM", "多米尼克"), ("DO", "多米尼加共和国"),
    ("EC", "厄瓜多尔"), ("EG", "埃及"), ("SV", "萨尔瓦多"), ("GQ", "赤道几内亚"),
    ("ER", "厄立特里亚"), ("EE", "爱沙尼亚"), ("SZ", "斯威士兰"), ("ET", "埃塞俄比亚"),
    ("FK", "福克兰群岛（马尔维纳斯）"), ("FO", "法罗群岛"), ("FJ", "斐济"), ("FI", "芬兰"),
    ("FR", "法国"), ("GF", "法属圭亚那"), ("PF", "法属波利尼西亚"), ("TF", "法属南部领地"),
    ("GA", "加蓬"), ("GM", "冈比亚"), ("GE", "格鲁吉亚"), ("DE", "德国"),
    ("GH", "加纳"), ("GI", "直布罗陀"), ("GR", "希腊"), ("GL", "格陵兰"),
    ("GD", "格林纳达"), ("GP", "瓜德罗普"), ("GU", "关岛"), ("GT", "危地马拉"),
    ("GG", "根西"), ("GN", "几内亚"), ("GW", "几内亚比绍"), ("GY", "圭亚那"),
    ("HT", "海地"), ("HM", "赫德岛和麦克唐纳群岛"), ("VA", "梵蒂冈"), ("HN", "洪都拉斯"),
    ("HK", "香港"), ("HU", "匈牙利"), ("IS", "冰岛"), ("IN", "印度"),
    ("ID", "印度尼西亚"), ("IR", "伊朗"), ("IQ", "伊拉克"), ("IE", "爱尔兰"),
    ("IM", "马恩岛"), ("IL", "以色列"), ("IT", "意大利"), ("JM", "牙买加"),
    ("JP", "日本"), ("JE", "泽西"), ("JO", "约旦"), ("KZ", "哈萨克斯坦"),
    ("KE", "肯尼亚"), ("KI", "基里巴斯"), ("KP", "朝鲜"), ("KR", "韩国"),
    ("KW", "科威特"), ("KG", "吉尔吉斯斯坦"), ("LA", "老挝人民民主共和国"), ("LV", "拉脱维亚"),
    ("LB", "黎巴嫩"), ("LS", "莱索托"), ("LR", "利比里亚"), ("LY", "利比亚"),
    ("LI", "列支敦士登"), ("LT", "立陶宛"), ("LU", "卢森堡"), ("MO", "澳门"),
    ("MG", "马达加斯加"), ("MW", "马拉维"), ("MY", "马来西亚"), ("MV", "马尔代夫"),
    ("ML", "马里"), ("MT", "马耳他"), ("MH", "马绍尔群岛"), ("MQ", "马提尼克"),
    ("MR", "毛里塔尼亚"), ("MU", "毛里求斯"), ("YT", "马约特"), ("MX", "墨西哥"),
    ("FM", "密克罗尼西亚联邦"), ("MD", "摩尔多瓦"), ("MC", "摩纳哥"), ("MN", "蒙古"),
    ("ME", "黑山"), ("MS", "蒙特塞拉特"), ("MA", "摩洛哥"), ("MZ", "莫桑比克"),
    ("MM", "缅甸"), ("NA", "纳米比亚"), ("NR", "瑙鲁"), ("NP", "尼泊尔"),
    ("NL", "荷兰"), ("NC", "新喀里多尼亚"), ("NZ", "新西兰"), ("NI", "尼加拉瓜"),
    ("NE", "尼日尔"), ("NG", "尼日利亚"), ("NU", "纽埃"), ("NF", "诺福克岛"),
    ("MK", "北马其顿"), ("MP", "北马里亚纳群岛"), ("NO", "挪威"), ("OM", "阿曼"),
    ("PK", "巴基斯坦"), ("PW", "帕劳"), ("PS", "巴勒斯坦"), ("PA", "巴拿马"),
    ("PG", "巴布亚新几内亚"), ("PY", "巴拉圭"), ("PE", "秘鲁"), ("PH", "菲律宾"),
    ("PN", "皮特凯恩"), ("PL", "波兰"), ("PT", "葡萄牙"), ("PR", "波多黎各"),
    ("QA", "卡塔尔"), ("RE", "留尼汪"), ("RO", "罗马尼亚"), ("RU", "俄罗斯"),
    ("RW", "卢旺达"), ("BL", "圣巴泰勒米"), ("SH", "圣赫勒拿、阿森松和特里斯坦-达库尼亚"),
    ("KN", "圣基茨和尼维斯"), ("LC", "圣卢西亚"), ("MF", "法属圣马丁"), ("PM", "圣皮埃尔和密克隆"),
    ("VC", "圣文森特和格林纳丁斯"), ("WS", "萨摩亚"), ("SM", "圣马力诺"), ("ST", "圣多美和普林西比"),
    ("SA", "沙特阿拉伯"), ("SN", "塞内加尔"), ("RS", "塞尔维亚"), ("SC", "塞舌尔"),
    ("SL", "塞拉利昂"), ("SG", "新加坡"), ("SX", "荷属圣马丁"), ("SK", "斯洛伐克"),
    ("SI", "斯洛文尼亚"), ("SB", "所罗门群岛"), ("SO", "索马里"), ("ZA", "南非"),
    ("GS", "南乔治亚和南桑威奇群岛"), ("SS", "南苏丹"), ("ES", "西班牙"), ("LK", "斯里兰卡"),
    ("SD", "苏丹"), ("SR", "苏里南"), ("SJ", "斯瓦尔巴和扬马延"), ("SE", "瑞典"),
    ("CH", "瑞士"), ("SY", "叙利亚"), ("TW", "台湾"), ("TJ", "塔吉克斯坦"),
    ("TZ", "坦桑尼亚"), ("TH", "泰国"), ("TL", "东帝汶"), ("TG", "多哥"),
    ("TK", "托克劳"), ("TO", "汤加"), ("TT", "特立尼达和多巴哥"), ("TN", "突尼斯"),
    ("TR", "土耳其"), ("TM", "土库曼斯坦"), ("TC", "特克斯和凯科斯群岛"), ("TV", "图瓦卢"),
    ("UG", "乌干达"), ("UA", "乌克兰"), ("AE", "阿拉伯联合酋长国"), ("GB", "英国"),
    ("UM", "美国本土外小岛屿"), ("US", "美国"), ("UY", "乌拉圭"), ("UZ", "乌兹别克斯坦"),
    ("VU", "瓦努阿图"), ("VE", "委内瑞拉"), ("VN", "越南"), ("VG", "英属维尔京群岛"),
    ("VI", "美属维尔京群岛"), ("WF", "瓦利斯和富图纳"), ("EH", "西撒哈拉"), ("YE", "也门"),
    ("ZM", "赞比亚"), ("ZW", "津巴布韦"),
];

static SUTRAS_CACHE: OnceLock<Vec<Sutra>> = OnceLock::new();
static PACKAGES_CACHE: OnceLock<Vec<DharmaPackage>> = OnceLock::new();
static COUNTRIES_CACHE: OnceLock<Vec<CountryNode>> = OnceLock::new();

pub fn all_sutras() -> &'static [Sutra] {
    SUTRAS_CACHE
        .get_or_init(|| {
            vec![
                Sutra {
                    id: "diamond_sutra".into(),
                    title: "金刚经".into(),
                    category: "般若部".into(),
                    content: "如是我闻。一时，佛在舍卫国祇树给孤独园，与大比丘众千二百五十人俱。佛告须菩提：诸菩萨摩诃萨应如是降伏其心！所有一切众生之类，我皆令入无余涅盘而灭度之。若菩萨有我相、人相、众生相、寿者相，即非菩萨。应无所住而生其心。一切有为法，如梦幻泡影，如露亦如电，应作如是观。".into(),
                },
                Sutra {
                    id: "heart_sutra".into(),
                    title: "心经".into(),
                    category: "般若部".into(),
                    content: "观自在菩萨，行深般若波罗蜜多时，照见五蕴皆空，度一切苦厄。舍利子，色不异空，空不异色，色即是空，空即是色，受想行识，亦复如是。无挂碍故，无有恐怖，远离颠倒梦想，究竟涅盘。揭谛揭谛，波罗揭谛，波罗僧揭谛，菩提萨婆诃。".into(),
                },
                Sutra {
                    id: "lotus_sutra".into(),
                    title: "法华经".into(),
                    category: "法华部".into(),
                    content: "如是我闻。一时，佛住王舍城耆阇崛山中，与大比丘众万二千人俱。诸佛世尊唯以一大事因缘故出现于世：欲令众生开佛知见使得清净故，欲示众生佛之知见故，欲令众生悟佛知见故，欲令众生入佛知见道故。".into(),
                },
                Sutra {
                    id: "avatamsaka_sutra".into(),
                    title: "华严经".into(),
                    category: "华严部".into(),
                    content: "如是我闻。一时，佛在摩竭提国阿兰若法菩提场中，始成正觉。智入三世悉皆平等，其身充满一切世间，其音普顺十方国土。心如工画师，能画诸世间，五蕴悉从生，无法而不造。不忘初心，方得始终。".into(),
                },
                Sutra {
                    id: "surangama_sutra".into(),
                    title: "楞严经".into(),
                    category: "密教部".into(),
                    content: "如是我闻。一时，佛在室罗筏城祇桓精舍，与大比丘众千二百五十人俱。狂心顿歇，歇即菩提。胜净明心，本周法界，不从人得。摄心为戒，因戒生定，因定发慧，是则名为三无漏学。".into(),
                },
                Sutra {
                    id: "yuanjue_sutra".into(),
                    title: "圆觉经".into(),
                    category: "经集部".into(),
                    content: "如是我闻。一时，婆伽婆入于神通大光明藏，三昧正受，一切如来光严住持，是诸众生清净觉地。知幻即离，不作方便；离幻即觉，亦无渐次。一切菩萨及末世众生，应当远离一切幻化虚妄境界。".into(),
                },
                Sutra {
                    id: "vimalakirti_sutra".into(),
                    title: "维摩诘经".into(),
                    category: "经集部".into(),
                    content: "如是我闻。一时，佛在毗耶离庵罗树园，与大比丘众八千人俱。若菩萨欲得净土，当净其心；随其心净，则佛土净。直心是道场，无虚假故。布施、持戒、忍辱、精进、禅定、智慧及方便力，无不具足。".into(),
                },
                Sutra {
                    id: "ksitigarbha_sutra".into(),
                    title: "地藏经".into(),
                    category: "地藏部".into(),
                    content: "如是我闻。一时，佛在忉利天，为母说法。地狱未空，誓不成佛；众生度尽，方证菩提。如来含笑，放百千万亿大光明云，所谓大圆满光明云、大慈悲光明云、大智慧光明云、大般若光明云。".into(),
                },
            ]
        })
        .as_slice()
}

pub fn all_dharma_packages() -> &'static [DharmaPackage] {
    PACKAGES_CACHE
        .get_or_init(|| {
            let parsed: R2FilesList = serde_json::from_str(R2_FILES_LIST_JSON).unwrap_or(R2FilesList {
                objects: Vec::new(),
                files: Vec::new(),
            });
            let items = if !parsed.objects.is_empty() {
                parsed.objects
            } else {
                parsed.files
            };
            items
                .into_iter()
                .map(|e| DharmaPackage {
                    display_name: e
                        .key
                        .trim()
                        .strip_suffix(".amitabha")
                        .unwrap_or(e.key.trim())
                        .trim()
                        .to_string(),
                    key: e.key,
                    size_bytes: e.size,
                    uploaded: e.uploaded,
                })
                .collect()
        })
        .as_slice()
}

fn fallback_geo(code: &str) -> Option<(&'static str, &'static str, f64, f64, &'static str)> {
    match code {
        "AQ" => Some(("Antarctica", "McMurdo", -75.250973, -0.071389, "Antarctica")),
        "BQ" => Some(("Caribbean Netherlands", "Kralendijk", 12.1784, -68.2385, "North America")),
        "BV" => Some(("Bouvet Island", "Bouvet", -54.4208, 3.3464, "Antarctica")),
        "GF" => Some(("French Guiana", "Cayenne", 4.9224, -52.3135, "South America")),
        "GP" => Some(("Guadeloupe", "Basse-Terre", 16.2650, -61.5510, "North America")),
        "HK" => Some(("Hong Kong", "Hong Kong", 22.3193, 114.1694, "Asia")),
        "HM" => Some(("Heard Island", "Atlas Cove", -53.0818, 73.5042, "Antarctica")),
        "MO" => Some(("Macau", "Macau", 22.1987, 113.5439, "Asia")),
        "MQ" => Some(("Martinique", "Fort-de-France", 14.6415, -61.0242, "North America")),
        "RE" => Some(("Reunion", "Saint-Denis", -21.1151, 55.5364, "Africa")),
        "YT" => Some(("Mayotte", "Mamoudzou", -12.8275, 45.1662, "Africa")),
        _ => None,
    }
}

pub fn all_countries() -> &'static [CountryNode] {
    COUNTRIES_CACHE
        .get_or_init(|| {
            let mut csv_map: HashMap<String, (String, String, f64, f64, String)> = HashMap::new();
            for line in CONCAP_CSV.lines().skip(1) {
                let parts: Vec<&str> = line.trim().split(',').collect();
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
                    (
                        parts[0].trim().to_string(),
                        parts[1].trim().to_string(),
                        lat,
                        lng,
                        parts[5].trim().to_string(),
                    ),
                );
            }

            let mut nodes = Vec::with_capacity(COUNTRY_NAMES.len() + 1);
            let mut seen = HashSet::new();
            for &(code, name_zh) in COUNTRY_NAMES {
                seen.insert(code);
                let (name_en, capital, lat, lng, continent) =
                    match (csv_map.get(code), fallback_geo(code)) {
                        (Some(r), Some(fb)) if r.2 == 0.0 && r.3 == 0.0 => (
                            r.0.clone(),
                            fb.1.to_string(),
                            fb.2,
                            fb.3,
                            r.4.clone(),
                        ),
                        (Some(r), _) => (r.0.clone(), r.1.clone(), r.2, r.3, r.4.clone()),
                        (None, Some(fb)) => (
                            fb.0.to_string(),
                            fb.1.to_string(),
                            fb.2,
                            fb.3,
                            fb.4.to_string(),
                        ),
                        (None, None) => (code.to_string(), String::new(), 0.0, 0.0, String::new()),
                    };
                let server_count = match code {
                    "AR" | "AU" | "BR" | "CA" | "CN" | "DE" | "DK" | "ES" | "FR" | "GB" | "IN"
                    | "IT" | "KR" | "MX" | "NL" | "RU" | "US" => 3,
                    _ => 2,
                };
                nodes.push(CountryNode {
                    code: code.to_string(),
                    name_zh: name_zh.to_string(),
                    name_en,
                    capital,
                    continent,
                    latitude: lat,
                    longitude: lng,
                    server_count,
                });
            }
            if let Some(ko) = csv_map.get("KO") {
                if !seen.contains("KO") {
                    nodes.push(CountryNode {
                        code: "KO".into(),
                        name_zh: "科索沃".into(),
                        name_en: ko.0.clone(),
                        capital: ko.1.clone(),
                        continent: ko.4.clone(),
                        latitude: ko.2,
                        longitude: ko.3,
                        server_count: 2,
                    });
                }
            }
            nodes.sort_by(|a, b| a.name_zh.cmp(&b.name_zh).then_with(|| a.code.cmp(&b.code)));
            nodes
        })
        .as_slice()
}

pub fn format_leaderboard_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut idx = 0usize;
    while value >= 1000.0 && idx < UNITS.len() - 1 {
        value /= 1000.0;
        idx += 1;
    }
    let mut s = if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    if let Some(stripped) = s.strip_suffix(".0") {
        s = stripped.to_string();
    }
    format!("{s} {}", UNITS[idx])
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementBadge {
    pub id: String,
    pub title: String,
    pub title_en: String,
    pub description: String,
    pub icon: String,
    pub tier: String,
    pub unlocked: bool,
    pub unlocked_at_ms: Option<i64>,
}

const ACHIEVEMENT_DEFS: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("daily_start", "今日已开启修行", "Daily Practice Begun", "迈出修行的第一步", "🌅", "bronze"),
    ("first_session", "初心不改", "Beginner's Mind", "完成首次修行", "✨", "gold"),
    ("streak_3", "三日精进", "3-Day Diligence", "连续修行3天", "🔥", "bronze"),
    ("streak_7", "七日不辍", "7-Day Steadfastness", "连续修行7天", "🌟", "silver"),
    ("streak_21", "习惯养成", "21-Day Habit", "连续修行21天", "💎", "gold"),
    ("streak_30", "一月精修", "30-Day Mastery", "连续修行30天", "🏆", "platinum"),
    ("streak_100", "百日筑基", "100-Day Foundation", "连续修行100天", "👑", "legendary"),
    ("midnight_practice", "子时修行", "Midnight Vigil", "在子时开始修行", "🌙", "silver"),
    ("morning_practice", "清晨早课", "Dawn Chanting", "在清晨开始修行", "🌄", "silver"),
    ("duration_5", "五分精进", "5-Minute Focus", "单次修行5分钟", "⏱️", "bronze"),
    ("duration_30", "半时功德", "Half-Hour Merit", "单次修行30分钟", "⏳", "silver"),
    ("duration_60", "一时圆满", "Full-Hour Perfection", "单次修行60分钟", "🕐", "gold"),
    ("chant_108", "一百零八遍", "108 Mala Chants", "单次念诵108遍", "📿", "gold"),
    ("chant_1000", "千遍精进", "1,000 Chants", "累计念诵1000遍", "🙏", "platinum"),
    ("chant_10000", "万遍圆满", "10,000 Chants", "累计念诵10000遍", "🪷", "legendary"),
    ("sessions_10", "十次精进", "10 Sessions", "累计修行10次", "🎯", "bronze"),
    ("sessions_50", "五十功德", "50 Sessions", "累计修行50次", "🎖️", "silver"),
    ("sessions_100", "百次圆满", "100 Sessions", "累计修行100次", "🏅", "gold"),
    ("total_60min", "累计一小时", "1 Cumulative Hour", "累计修行60分钟", "⌛", "bronze"),
    ("total_10hr", "累计十小时", "10 Cumulative Hours", "累计修行10小时", "🌟", "silver"),
    ("total_100hr", "累计百时", "100 Cumulative Hours", "累计修行100小时", "💫", "platinum"),
];

pub fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS fabushi_offerings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            country_code TEXT NOT NULL,
            package_key TEXT NOT NULL,
            bytes_offered INTEGER NOT NULL,
            merit_earned INTEGER NOT NULL,
            dedication TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS fabushi_zen_state (
            singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
            wheel_rotations INTEGER NOT NULL DEFAULT 0,
            total_merit INTEGER NOT NULL DEFAULT 0,
            total_sessions INTEGER NOT NULL DEFAULT 0,
            total_minutes INTEGER NOT NULL DEFAULT 0,
            total_chants INTEGER NOT NULL DEFAULT 0,
            consecutive_days INTEGER NOT NULL DEFAULT 0,
            last_session_day INTEGER,
            unlocked_json TEXT NOT NULL DEFAULT '{}'
        );
        INSERT OR IGNORE INTO fabushi_zen_state (singleton_id) VALUES (1);
        CREATE TABLE IF NOT EXISTS fabushi_practice_sessions (
            session_id TEXT PRIMARY KEY,
            sutra_id TEXT NOT NULL,
            duration_seconds INTEGER NOT NULL,
            chant_count INTEGER NOT NULL,
            dedication TEXT NOT NULL,
            created_at_ms INTEGER NOT NULL
        );",
    )
    .map_err(|e| format!("Failed to initialize fabushi_miniapp schema: {e}"))
}

fn load_zen_state(
    conn: &Connection,
) -> Result<(u64, u64, u64, u64, u64, u64, Option<i64>, BTreeMap<String, i64>), String> {
    conn.query_row(
        "SELECT wheel_rotations, total_merit, total_sessions, total_minutes, total_chants, consecutive_days, last_session_day, unlocked_json
         FROM fabushi_zen_state WHERE singleton_id = 1",
        [],
        |r| {
            let raw: String = r.get(7)?;
            let map: BTreeMap<String, i64> = serde_json::from_str(&raw).unwrap_or_default();
            Ok((
                r.get::<_, i64>(0)? as u64,
                r.get::<_, i64>(1)? as u64,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, i64>(3)? as u64,
                r.get::<_, i64>(4)? as u64,
                r.get::<_, i64>(5)? as u64,
                r.get::<_, Option<i64>>(6)?,
                map,
            ))
        },
    )
    .map_err(|e| e.to_string())
}

pub fn handle_rpc(conn: &Connection, submethod: &str, params: &Value) -> Result<Value, String> {
    init_schema(conn)?;
    match submethod {
        "bootstrap" => {
            let (rotations, merit, sessions, minutes, chants, streak, _, unlocked) =
                load_zen_state(conn)?;
            let offered_countries: i64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT country_code) FROM fabushi_offerings",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let total_bytes: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(bytes_offered), 0) FROM fabushi_offerings",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            Ok(json!({
                "miniAppId": "global-dharma",
                "title": "全球法布施",
                "sutrasCount": all_sutras().len(),
                "packagesCount": all_dharma_packages().len(),
                "countriesCount": all_countries().len(),
                "offeredCountriesCount": offered_countries,
                "totalBytesOffered": total_bytes,
                "totalBytesFormatted": format_leaderboard_bytes(total_bytes.max(0) as u64),
                "zenRoom": {
                    "wheelRotations": rotations,
                    "totalMerit": merit,
                    "totalSessions": sessions,
                    "totalMinutes": minutes,
                    "totalChants": chants,
                    "consecutiveDays": streak,
                    "unlockedAchievements": unlocked.len(),
                }
            }))
        }

        "sutras" => Ok(json!({
            "sutras": all_sutras(),
            "packages": all_dharma_packages(),
        })),

        "countries" => {
            let query = params
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let mut offered_set = BTreeSet::new();
            if let Ok(mut stmt) = conn.prepare("SELECT DISTINCT country_code FROM fabushi_offerings") {
                if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                    for c in rows.flatten() {
                        offered_set.insert(c);
                    }
                }
            }
            let list: Vec<Value> = all_countries()
                .iter()
                .filter(|c| {
                    query.is_empty()
                        || c.code.to_ascii_lowercase().contains(&query)
                        || c.name_zh.contains(&query)
                        || c.name_en.to_ascii_lowercase().contains(&query)
                        || c.capital.to_ascii_lowercase().contains(&query)
                })
                .map(|c| {
                    json!({
                        "code": c.code,
                        "nameZh": c.name_zh,
                        "nameEn": c.name_en,
                        "capital": c.capital,
                        "continent": c.continent,
                        "latitude": c.latitude,
                        "longitude": c.longitude,
                        "serverCount": c.server_count,
                        "illuminated": offered_set.contains(&c.code),
                    })
                })
                .collect();
            Ok(json!({
                "total": list.len(),
                "countries": list,
            }))
        }

        "offerDharma" => {
            let country_code = params
                .get("countryCode")
                .and_then(Value::as_str)
                .unwrap_or("CN")
                .trim()
                .to_ascii_uppercase();
            let country = all_countries()
                .iter()
                .find(|c| c.code == country_code)
                .ok_or_else(|| format!("Unknown country code '{country_code}'"))?;
            let package_key = params
                .get("packageKey")
                .and_then(Value::as_str)
                .unwrap_or("ritual_file.zip");
            let pkg = all_dharma_packages()
                .iter()
                .find(|p| p.key == package_key || p.display_name == package_key)
                .or_else(|| all_dharma_packages().first());
            let bytes = pkg.map(|p| p.size_bytes).unwrap_or(108_000_000);
            let merit_delta = 108i64 * (country.server_count as i64);
            let dedication = params
                .get("dedication")
                .and_then(Value::as_str)
                .unwrap_or("愿以此功德，普及于一切，我等与众生，皆共成佛道。");
            let now = chrono::Utc::now().timestamp_millis();

            conn.execute(
                "INSERT INTO fabushi_offerings (country_code, package_key, bytes_offered, merit_earned, dedication, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![country.code, package_key, bytes as i64, merit_delta, dedication, now],
            )
            .map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE fabushi_zen_state SET total_merit = total_merit + ?1 WHERE singleton_id = 1",
                params![merit_delta],
            )
            .map_err(|e| e.to_string())?;

            let (_, total_merit, ..) = load_zen_state(conn)?;
            Ok(json!({
                "country": country,
                "packageKey": package_key,
                "bytesOffered": bytes,
                "bytesFormatted": format_leaderboard_bytes(bytes),
                "meritEarned": merit_delta,
                "totalMerit": total_merit,
                "dedication": dedication,
            }))
        }

        "spinPrayerWheel" => {
            let spins = params
                .get("rotations")
                .or_else(|| params.get("spins"))
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 10_800);
            let merit_delta = spins * 9;
            conn.execute(
                "UPDATE fabushi_zen_state
                 SET wheel_rotations = wheel_rotations + ?1,
                     total_merit = total_merit + ?2,
                     total_chants = total_chants + ?1
                 WHERE singleton_id = 1",
                params![spins as i64, merit_delta as i64],
            )
            .map_err(|e| e.to_string())?;
            let (rotations, total_merit, _, _, total_chants, ..) = load_zen_state(conn)?;
            Ok(json!({
                "spinsAdded": spins,
                "wheelRotations": rotations,
                "meritAdded": merit_delta,
                "totalMerit": total_merit,
                "totalChants": total_chants,
                "mantra": "嗡嘛呢呗美吽 (Om Mani Padme Hum)",
            }))
        }

        "recordPractice" => {
            let sutra_id = params
                .get("sutraId")
                .or_else(|| params.get("sutra"))
                .and_then(Value::as_str)
                .unwrap_or("heart_sutra");
            let duration_seconds = params
                .get("durationSeconds")
                .and_then(Value::as_u64)
                .unwrap_or(300);
            let chant_count = params
                .get("chantCount")
                .and_then(Value::as_u64)
                .unwrap_or(108);
            let dedication = params
                .get("dedication")
                .and_then(Value::as_str)
                .unwrap_or("愿消三障诸烦恼，愿得智慧真明了。");
            let now = params
                .get("timestampMs")
                .and_then(Value::as_i64)
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            let tz_offset_min = params
                .get("tzOffsetMinutes")
                .and_then(Value::as_i64)
                .unwrap_or(480);

            let (rotations, mut merit, mut sessions, mut minutes, mut chants, mut streak, last_day, mut unlocked) =
                load_zen_state(conn)?;

            let local_secs = (now / 1000) + tz_offset_min * 60;
            let today = local_secs.div_euclid(86_400);
            let hour = (local_secs.rem_euclid(86_400) / 3600) as u32;

            let mut newly_unlocked = Vec::new();
            let mut try_unlock = |id: &str| {
                if !unlocked.contains_key(id) {
                    unlocked.insert(id.to_string(), now);
                    newly_unlocked.push(id.to_string());
                }
            };

            if last_day != Some(today) {
                try_unlock("daily_start");
                streak = match last_day {
                    Some(prev) if prev == today - 1 => streak + 1,
                    _ => 1,
                };
            }
            if hour >= 23 || hour < 1 {
                try_unlock("midnight_practice");
            }
            if (5..7).contains(&hour) {
                try_unlock("morning_practice");
            }

            sessions += 1;
            let dur_min = duration_seconds / 60;
            minutes += dur_min;
            chants += chant_count;
            merit += 36 + chant_count + dur_min * 6;

            if sessions >= 1 {
                try_unlock("first_session");
            }
            for &(d, id) in &[(3, "streak_3"), (7, "streak_7"), (21, "streak_21"), (30, "streak_30"), (100, "streak_100")] {
                if streak >= d {
                    try_unlock(id);
                }
            }
            for &(m, id) in &[(5, "duration_5"), (30, "duration_30"), (60, "duration_60")] {
                if dur_min >= m {
                    try_unlock(id);
                }
            }
            if chant_count >= 108 {
                try_unlock("chant_108");
            }
            if chants >= 1000 {
                try_unlock("chant_1000");
            }
            if chants >= 10000 {
                try_unlock("chant_10000");
            }
            for &(s, id) in &[(10, "sessions_10"), (50, "sessions_50"), (100, "sessions_100")] {
                if sessions >= s {
                    try_unlock(id);
                }
            }
            for &(tm, id) in &[(60, "total_60min"), (600, "total_10hr"), (6000, "total_100hr")] {
                if minutes >= tm {
                    try_unlock(id);
                }
            }

            let session_id = format!("zen_{}", uuid::Uuid::new_v4().simple());
            conn.execute(
                "INSERT INTO fabushi_practice_sessions (session_id, sutra_id, duration_seconds, chant_count, dedication, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![session_id, sutra_id, duration_seconds as i64, chant_count as i64, dedication, now],
            )
            .map_err(|e| e.to_string())?;

            let unlocked_json = serde_json::to_string(&unlocked).map_err(|e| e.to_string())?;
            conn.execute(
                "UPDATE fabushi_zen_state
                 SET total_merit = ?1, total_sessions = ?2, total_minutes = ?3, total_chants = ?4,
                     consecutive_days = ?5, last_session_day = ?6, unlocked_json = ?7
                 WHERE singleton_id = 1",
                params![
                    merit as i64,
                    sessions as i64,
                    minutes as i64,
                    chants as i64,
                    streak as i64,
                    today,
                    unlocked_json,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok(json!({
                "sessionId": session_id,
                "wheelRotations": rotations,
                "totalMerit": merit,
                "totalSessions": sessions,
                "totalMinutes": minutes,
                "totalChants": chants,
                "consecutiveDays": streak,
                "newlyUnlocked": newly_unlocked,
            }))
        }

        "achievements" => {
            let (_, _, sessions, minutes, chants, streak, _, unlocked) = load_zen_state(conn)?;
            let catalog: Vec<AchievementBadge> = ACHIEVEMENT_DEFS
                .iter()
                .map(|&(id, title, title_en, desc, icon, tier)| AchievementBadge {
                    id: id.into(),
                    title: title.into(),
                    title_en: title_en.into(),
                    description: desc.into(),
                    icon: icon.into(),
                    tier: tier.into(),
                    unlocked: unlocked.contains_key(id),
                    unlocked_at_ms: unlocked.get(id).copied(),
                })
                .collect();
            Ok(json!({
                "totalSessions": sessions,
                "totalMinutes": minutes,
                "totalChants": chants,
                "consecutiveDays": streak,
                "unlockedCount": unlocked.len(),
                "totalBadges": catalog.len(),
                "achievements": catalog,
            }))
        }

        "leaderboard" => {
            let (_, my_merit, ..) = load_zen_state(conn)?;
            let my_bytes: u64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(bytes_offered), 0) FROM fabushi_offerings",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                .max(0) as u64;
            let my_countries: u64 = conn
                .query_row(
                    "SELECT COUNT(DISTINCT country_code) FROM fabushi_offerings",
                    [],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0)
                .max(0) as u64;

            let mut entries = vec![
                ("莲池海会·智观", 189u64, 128_500_000_000u64, 108_900u64),
                ("般若行者·明心", 142u64, 84_200_000_000u64, 76_400u64),
                ("普贤愿海·净行", 96u64, 42_800_000_000u64, 48_200u64),
                ("本机行者 (You)", my_countries.max(1), my_bytes, my_merit),
            ];
            entries.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| b.3.cmp(&a.3)));

            let ranked: Vec<Value> = entries
                .into_iter()
                .enumerate()
                .map(|(idx, (name, countries, bytes, merit))| {
                    json!({
                        "rank": idx + 1,
                        "practitioner": name,
                        "countriesIlluminated": countries,
                        "bytesOffered": bytes,
                        "bytesFormatted": format_leaderboard_bytes(bytes),
                        "merit": merit,
                    })
                })
                .collect();
            Ok(json!({ "leaderboard": ranked }))
        }

        "aiDharmaChat" => {
            let prompt = params
                .get("prompt")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("如何安住当下？");
            let (sutra_title, reply) = if prompt.contains("空") || prompt.contains("心经") {
                (
                    "心经",
                    "《心经》云：「照见五蕴皆空，度一切苦厄。」观照念头来去而不执取，心无挂碍，当下即是清净安稳处。",
                )
            } else if prompt.contains("地藏") || prompt.contains("愿") {
                (
                    "地藏经",
                    "《地藏经》以大孝大愿为根本：「众生度尽，方证菩提。」于日用细行中常怀慈悲利他之心，便是广大回向。",
                )
            } else {
                (
                    "金刚经",
                    "《金刚经》示导：「应无所住而生其心。」在尘劳事务中尽心尽力，而内心不留挂碍痕迹，便是最踏实的修行。",
                )
            };
            Ok(json!({
                "prompt": prompt,
                "citedSutra": sutra_title,
                "reply": reply,
            }))
        }

        other => Err(format!("Unsupported fabushiMiniapp method: {other}")),
    }
}
