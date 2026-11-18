use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

const R2_FILES_LIST_JSON: &str = include_str!("../assets/r2-files-list.json");

/// Canonical Buddhist sutra entry ported verbatim from `sutra_model.dart`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sutra {
    pub id: String,
    pub title: String,
    pub category: String,
    pub content: String,
}

/// Dharma ritual package entry parsed from `r2-files-list.json`.
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

static SUTRAS_CACHE: OnceLock<Vec<Sutra>> = OnceLock::new();
static PACKAGES_CACHE: OnceLock<Vec<DharmaPackage>> = OnceLock::new();

fn build_sutras() -> Vec<Sutra> {
    vec![
        Sutra {
            id: "diamond_sutra".to_string(),
            title: "金刚经".to_string(),
            category: "般若部".to_string(),
            content: concat!(
                "如是我闻。一时，佛在舍卫国祇树给孤独园，与大比丘众千二百五十人俱。\n\n",
                "尔时，世尊食时，著衣持钵，入舍卫大城乞食。于其城中，次第乞已，还至本处。饭食讫，收衣钵，洗足已，敷座而坐。\n\n",
                "时，长老须菩提在大众中即从座起，偏袒右肩，右膝著地，合掌恭敬而白佛言：\"希有！世尊！如来善护念诸菩萨，善付嘱诸菩萨。世尊！善男子、善女人，发阿耨多罗三藐三菩提心，应云何住，云何降伏其心？\"\n\n",
                "佛言：\"善哉，善哉。须菩提！如汝所说：如来善护念诸菩萨，善付嘱诸菩萨。汝今谛听！当为汝说：善男子、善女人，发阿耨多罗三藐三菩提心，应如是住，如是降伏其心。\"\n\n",
                "\"唯然。世尊！愿乐欲闻。\"\n\n",
                "佛告须菩提：\"诸菩萨摩诃萨应如是降伏其心！所有一切众生之类：若卵生、若胎生、若湿生、若化生；若有色、若无色；若有想、若无想、若非有想非无想，我皆令入无余涅盘而灭度之。如是灭度无量无数无边众生，实无众生得灭度者。\"\n\n",
                "\"何以故？须菩提！若菩萨有我相、人相、众生相、寿者相，即非菩萨。\""
            )
            .to_string(),
        },
        Sutra {
            id: "heart_sutra".to_string(),
            title: "心经".to_string(),
            category: "般若部".to_string(),
            content: concat!(
                "观自在菩萨，行深般若波罗蜜多时，照见五蕴皆空，度一切苦厄。\n\n",
                "舍利子，色不异空，空不异色，色即是空，空即是色，受想行识，亦复如是。\n\n",
                "舍利子，是诸法空相，不生不灭，不垢不净，不增不减。是故空中无色，无受想行识，无眼耳鼻舌身意，无色声香味触法，无眼界，乃至无意识界，无无明，亦无无明尽，乃至无老死，亦无老死尽。无苦集灭道，无智亦无得。\n\n",
                "以无所得故。菩提萨埵，依般若波罗蜜多故，心无挂碍。无挂碍故，无有恐怖，远离颠倒梦想，究竟涅盘。\n\n",
                "三世诸佛，依般若波罗蜜多故，得阿耨多罗三藐三菩提。\n\n",
                "故知般若波罗蜜多，是大神咒，是大明咒，是无上咒，是无等等咒，能除一切苦，真实不虚。\n\n",
                "故说般若波罗蜜多咒，即说咒曰：揭谛揭谛，波罗揭谛，波罗僧揭谛，菩提萨婆诃。"
            )
            .to_string(),
        },
        Sutra {
            id: "lotus_sutra".to_string(),
            title: "法华经".to_string(),
            category: "法华部".to_string(),
            content: concat!(
                "如是我闻。一时，佛住王舍城耆阇崛山中，与大比丘众万二千人俱。皆是阿罗汉，诸漏已尽，无复烦恼，逮得己利，尽诸有结，心得自在。\n\n",
                "其名曰：阿若憍陈如、摩诃迦叶、优楼频螺迦叶、伽耶迦叶、那提迦叶、舍利弗、大目犍连、摩诃迦旃延、阿㝹楼驮、劫宾那、憍梵波提、离婆多、毕陵伽婆蹉、薄拘罗、摩诃拘絺罗、难陀、孙陀罗难陀、富楼那弥多罗尼子、须菩提、阿难、罗睺罗，如是众所知识大阿罗汉等。\n\n",
                "复有学无学二千人。摩诃波阇波提比丘尼，与眷属六千人俱。罗睺罗母耶输陀罗比丘尼，亦与眷属俱。\n\n",
                "菩萨摩诃萨八万人，皆于阿耨多罗三藐三菩提不退转，皆得陀罗尼，乐说辩才，转不退转法轮，供养无量百千诸佛，于诸佛所植众德本，常为诸佛之所称叹，以慈修身，善入佛慧，通达大智，到于彼岸，名称普闻无量世界，能度无数百千众生。"
            )
            .to_string(),
        },
        Sutra {
            id: "avatamsaka_sutra".to_string(),
            title: "华严经".to_string(),
            category: "华严部".to_string(),
            content: concat!(
                "如是我闻。一时，佛在摩竭提国阿兰若法菩提场中，始成正觉。\n\n",
                "其地坚固，金刚所成，上妙宝轮及众宝华、清净摩尼以为严饰，诸色相海无边显现；摩尼为幢，常放光明，恒出妙音；众宝罗网、妙香华缨周匝垂布；摩尼宝王变现自在，雨无尽宝及众妙华分散于地；宝树行列，枝叶光茂。\n\n",
                "佛神力故，令此道场一切庄严于中影现。其菩提树高显殊特，金刚为身，琉璃为干；众杂妙宝以为枝条；宝叶扶疏，垂荫如云；宝华杂色，分枝布影；复以摩尼而为其果，含辉发焰，与华间列。\n\n",
                "尔时，世尊处于此座，于一切法成最正觉，智入三世悉皆平等，其身充满一切世间，其音普顺十方国土。譬如虚空具含众像，于诸境界无所分别；又如虚空普遍一切，于诸国土平等随入。"
            )
            .to_string(),
        },
        Sutra {
            id: "surangama_sutra".to_string(),
            title: "楞严经".to_string(),
            category: "密教部".to_string(),
            content: concat!(
                "如是我闻。一时，佛在室罗筏城祇桓精舍，与大比丘众千二百五十人俱。皆是无漏大阿罗汉，佛子住持，善超诸有。能于国土，成就威仪。从佛转轮，妙堪遗嘱，严净毗尼，弘范三界。应身无量，度脱众生。拔济未来，越诸尘累。\n\n",
                "其名曰：大智舍利弗、摩诃目犍连、摩诃拘絺罗、富楼那弥多罗尼子、须菩提、优波尼沙陀等，而为上首。\n\n",
                "复有无量辟支无学，并其初心，同来佛所。属诸比丘休夏自恣。十方菩萨咨决心疑，钦奉慈严，将求密义。\n\n",
                "即时，如来敷座晏安，为诸会中宣示深奥。法筵清众，得未曾有。迦陵仙音，遍十方界。恒沙菩萨来聚道场，文殊师利而为上首。"
            )
            .to_string(),
        },
        Sutra {
            id: "yuanjue_sutra".to_string(),
            title: "圆觉经".to_string(),
            category: "经集部".to_string(),
            content: concat!(
                "如是我闻。一时，婆伽婆入于神通大光明藏，三昧正受，一切如来光严住持，是诸众生清净觉地。身心寂灭，平等本际，圆满十方，不二随顺，于不二境现诸净土。\n\n",
                "与大菩萨摩诃萨十万人俱，其名曰：文殊师利菩萨、普贤菩萨、普眼菩萨、金刚藏菩萨、弥勒菩萨、清净慧菩萨、威德自在菩萨、辩音菩萨、净诸业障菩萨、普觉菩萨、圆觉菩萨、贤善首菩萨等而为上首，与诸眷属皆入三昧，同住如来平等法会。\n\n",
                "于是，文殊师利菩萨在大众中，即从座起，顶礼佛足，右绕三匝，长跪叉手而白佛言：\"大悲世尊！愿为此会诸来法众，说于如来本起清净因地法行，及说菩萨于大乘中发清净心，远离诸病，能使未来末世众生求大乘者不堕邪见。\"作是语已，五体投地，如是三请，终而复始。"
            )
            .to_string(),
        },
        Sutra {
            id: "vimalakirti_sutra".to_string(),
            title: "维摩诘经".to_string(),
            category: "经集部".to_string(),
            content: concat!(
                "如是我闻。一时，佛在毗耶离庵罗树园，与大比丘众八千人俱。菩萨三万二千，众所知识，大智本行皆悉成就，诸佛威神之所建立，为护法城受持正法。能师子吼，名闻十方。众人不请，友而安之，绍隆三宝能使不绝。\n\n",
                "降伏魔怨，制诸外道。悉已清净，永离盖缠。心常安住无碍解脱。念定总持，辩才不断。布施、持戒、忍辱、精进、禅定、智慧及方便力，无不具足。逮无所得不起法忍。\n\n",
                "已能随顺转不退轮。善解法相，知众生根，盖诸大众，得无所畏。功德智慧以修其心，相好严身色像第一，舍诸世间所有饰好。名称高远，踰于须弥。深信坚固，犹若金刚。法宝普照而雨甘露。"
            )
            .to_string(),
        },
        Sutra {
            id: "ksitigarbha_sutra".to_string(),
            title: "地藏经".to_string(),
            category: "地藏部".to_string(),
            content: concat!(
                "如是我闻。一时，佛在忉利天，为母说法。\n\n",
                "尔时，十方无量世界，不可说不可说一切诸佛，及大菩萨摩诃萨，皆来集会。赞叹释迦牟尼佛，能于五浊恶世，现不可思议大智慧神通之力，调伏刚强众生，知苦乐法。各遣侍者，问讯世尊。\n\n",
                "是时，如来含笑，放百千万亿大光明云，所谓大圆满光明云、大慈悲光明云、大智慧光明云、大般若光明云、大三昧光明云、大吉祥光明云、大福德光明云、大功德光明云、大归依光明云、大赞叹光明云，放如是等不可说光明云已。\n\n",
                "又出种种微妙之音，所谓檀波罗蜜音、尸波罗蜜音、羼提波罗蜜音、毗离耶波罗蜜音、禅波罗蜜音、般若波罗蜜音、慈悲音、喜舍音、解脱音、无漏音、智慧音、大智慧音、师子吼音、大师子吼音、云雷音、大云雷音。"
            )
            .to_string(),
        },
    ]
}

fn clean_package_display_name(key: &str) -> String {
    let trimmed = key.trim();
    if let Some(stripped) = trimmed.strip_suffix(".amitabha") {
        return stripped.trim().to_string();
    }
    trimmed.to_string()
}

fn build_packages() -> Vec<DharmaPackage> {
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
        .map(|entry| DharmaPackage {
            display_name: clean_package_display_name(&entry.key),
            key: entry.key,
            size_bytes: entry.size,
            uploaded: entry.uploaded,
        })
        .collect()
}

/// Returns all 8 canonical sutras.
pub fn all_sutras() -> &'static [Sutra] {
    SUTRAS_CACHE.get_or_init(build_sutras).as_slice()
}

/// Looks up a canonical sutra by title (or ID).
pub fn sutra_by_title(title: &str) -> Option<&'static Sutra> {
    let t = title.trim();
    all_sutras().iter().find(|s| s.title == t || s.id == t)
}

/// Returns all 12 Dharma ritual packages parsed from `r2-files-list.json`.
pub fn all_dharma_packages() -> &'static [DharmaPackage] {
    PACKAGES_CACHE.get_or_init(build_packages).as_slice()
}

/// Looks up a Dharma package by its exact key or display name.
pub fn dharma_package_by_key(key: &str) -> Option<&'static DharmaPackage> {
    let k = key.trim();
    all_dharma_packages()
        .iter()
        .find(|p| p.key == k || p.display_name == k)
}

/// Formats byte totals for the leaderboard using SI base-1000 units,
/// matching `formatLeaderboardBytes` in `leaderboard_screen.dart`.
pub fn format_leaderboard_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes as f64;
    let mut unit_index = 0usize;

    while value >= 1000.0 && unit_index < UNITS.len() - 1 {
        value /= 1000.0;
        unit_index += 1;
    }

    let mut formatted = if value >= 100.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };
    if let Some(stripped) = formatted.strip_suffix(".0") {
        formatted = stripped.to_string();
    }
    format!("{formatted} {}", UNITS[unit_index])
}

/// Formats file sizes using IEC base-1024 units, matching `FileTransferModel.getFileSizeString`.
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Formats minutes in Chinese (`0 分钟`, `X 分钟`, `Y 小时`, `Y 小时 Z 分钟`).
pub fn format_duration_zh(minutes: u32) -> String {
    if minutes == 0 {
        "0 分钟".to_string()
    } else if minutes < 60 {
        format!("{minutes} 分钟")
    } else {
        let hours = minutes / 60;
        let remain = minutes % 60;
        if remain == 0 {
            format!("{hours} 小时")
        } else {
            format!("{hours} 小时 {remain} 分钟")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_8_canonical_sutras_and_12_packages() {
        let sutras = all_sutras();
        assert_eq!(sutras.len(), 8);
        assert!(sutra_by_title("金刚经").unwrap().content.contains("须菩提"));
        assert!(sutra_by_title("心经").unwrap().content.contains("揭谛揭谛"));
        assert!(sutra_by_title("地藏经").unwrap().content.contains("忉利天"));

        let packages = all_dharma_packages();
        assert_eq!(packages.len(), 12);
        let ritual = dharma_package_by_key("ritual_file.zip").expect("ritual_file.zip present");
        assert_eq!(ritual.size_bytes, 832_697_691);
    }

    #[test]
    fn formats_leaderboard_bytes_like_flutter() {
        assert_eq!(format_leaderboard_bytes(0), "0 B");
        assert_eq!(format_leaderboard_bytes(999), "999 B");
        assert_eq!(format_leaderboard_bytes(1000), "1 KB");
        assert_eq!(format_leaderboard_bytes(1500), "1.5 KB");
        assert_eq!(format_leaderboard_bytes(128_500_000_000), "128 GB");
        assert_eq!(format_leaderboard_bytes(832_697_691), "833 MB");
    }
}
