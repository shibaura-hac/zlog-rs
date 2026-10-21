//! Scoring of small logs; expected values follow the Delphi code cited per test.

use super::*;
use crate::cty::tests::FIXTURE;
use zlog_core::rescore;

fn rules(kind: WorldContest, mode: ContestMode) -> WorldRules {
    WorldRules::new(kind, mode, Arc::new(Cty::parse(FIXTURE)))
}

fn q(call: &str, band: Band, mode: Mode, nr: &str) -> Qso {
    Qso {
        callsign: call.into(),
        band,
        mode,
        nr_rcvd: nr.into(),
        ..Qso::default()
    }
}

fn log(me: &str, qsos: Vec<Qso>) -> Log {
    let mut l = Log::default();
    l.header.callsign = me.into();
    l.qsos = qsos;
    l
}

fn run(kind: WorldContest, mode: ContestMode, l: &mut Log) -> (WorldRules, ScoreSheet) {
    let mut c = rules(kind, mode);
    let sheet = rescore(l, &mut c).unwrap();
    (c, sheet)
}

fn totals(s: &ScoreSheet) -> (u32, u32, u32, u64) {
    (s.total.qsos, s.total.points, s.total.multi, s.score)
}

#[test]
fn texts_complete() {
    assert!(texts().missing().is_empty());
    assert_eq!(Cqww.caption("en"), "CQ WW");
    assert_eq!(Cqww.group("en"), "World");
    assert!(WorldContest::ALL
        .iter()
        .all(|c| c.uses_utc() == (*c != Pedi)));
}

#[test]
fn cqww() {
    // UWWMulti.pas TWWMulti.AddNoUpdate: JA-JA 0, AS-AS 1, other continent 3;
    // zones and countries per band; UWWScore.pas: points x (zones + countries).
    let mut l = log(
        "JA1ZLO",
        vec![
            q("JA2AAA", Band::B14, Mode::Cw, "25"), // own country: 0, zone 25 + JA
            q("BY1AA", Band::B14, Mode::Cw, "24"),  // AS: 1, zone 24 + BY
            q("W1AW", Band::B14, Mode::Cw, "5"),    // NA: 3, zone 5 + K
            q("W1AW", Band::B14, Mode::Cw, "5"),    // dupe
            q("K6XX", Band::B21, Mode::Cw, "3"),    // 3, zone 3 + K on 21
            q("DL1AA", Band::B14, Mode::Ssb, "14"), // phone in a CW log: no score
        ],
    );
    let (_, s) = run(Cqww, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (5, 7, 8, 56));
    assert!(l.qsos[3].dupe);
    assert_eq!(l.qsos[2].multi2, "K");
}

#[test]
fn cqww_rtty_states() {
    // UWWMulti.pas AddNoUpdate (RTTY points) and UARRLDXMulti.pas AddMulti3NoUpdate.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Rtty, "5CT"),
            q("K1AR", Band::B14, Mode::Rtty, "5MA"),
            q("JA2AAA", Band::B14, Mode::Rtty, "25"),
        ],
    );
    let (c, s) = run(Cqww, ContestMode::Rtty, &mut l);
    // 3 + 3 + 1 points; zones 5, 25; K, JA; states CT, MA.
    assert_eq!(totals(&s), (3, 7, 6, 42));
    assert_eq!(l.qsos[1].multi3, "MA");
    assert!(c.valid_number(&q("W1AW", Band::B14, Mode::Rtty, "5CT")));
    assert!(!c.valid_number(&q("W1AW", Band::B14, Mode::Rtty, "5")));
    assert!(c.valid_number(&q("DL1AA", Band::B14, Mode::Rtty, "14")));
}

#[test]
fn wpx() {
    // UWPXMulti.pas AddNoUpdate: other continent 6 on 40 m, 3 on 20 m; same
    // continent other country 2/1; own country 1. Prefixes once per contest.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B7, Mode::Cw, "001"),
            q("W1AW", Band::B14, Mode::Cw, "002"),
            q("BY1AA", Band::B7, Mode::Cw, "003"),
            q("JA2AAA", Band::B14, Mode::Cw, "004"),
            q("JA1BBB", Band::B21, Mode::Cw, "005"),
        ],
    );
    l.qsos[4].nr_sent = "005".into();
    let (c, s) = run(Wpx, ContestMode::Cw, &mut l);
    // 6 + 3 + 2 + 1 + 1 = 13; prefixes W1, BY1, JA2, JA1.
    assert_eq!(totals(&s), (5, 13, 4, 52));
    assert!(!l.qsos[1].new_multi1);
    assert_eq!(c.next_serial(), 6);
    let st = Station::default();
    assert_eq!(c.sent_number(Band::B7, Power::P100, &st), "006");
    assert_eq!(c.exchange_from(Band::B7, &l.qsos[0]), None);
}

#[test]
fn iaru() {
    // UIARUMulti.pas AddNoUpdate: own zone or HQ 1, same continent 3, else 5.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("JA2AAA", Band::B14, Mode::Cw, "45"),
            q("JA2AAA", Band::B14, Mode::Ssb, "45"), // other mode: not a dupe
            q("BY1AA", Band::B14, Mode::Cw, "44"),
            q("W1AW", Band::B14, Mode::Cw, "ARRL"),
            q("DL1AA", Band::B21, Mode::Cw, "28"),
        ],
    );
    let (_, s) = run(Iaru, ContestMode::Mix, &mut l);
    // 1 + 1 + 3 + 1 + 5 = 11; 45, 44, ARRL on 14, 28 on 21.
    assert_eq!(totals(&s), (5, 11, 4, 44));
    assert!(!l.qsos[1].dupe);
}

#[test]
fn arrl_dx() {
    // UARRLDXMulti.pas (states per band), UARRLDXScore.pas (3 points).
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Cw, "CT"),
            q("K1AR", Band::B14, Mode::Cw, "MA"),
            q("K1XX", Band::B14, Mode::Cw, "CT"),
            q("W1AW", Band::B21, Mode::Cw, "CT"),
        ],
    );
    let (c, s) = run(ArrlDxDx, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (4, 12, 3, 36));
    assert!(!c.valid_number(&q("W1AW", Band::B14, Mode::Cw, "XX")));
    // UARRLWMulti.pas (countries per band) for the W/VE side.
    let mut l = log(
        "W1AW",
        vec![
            q("JA1ZLO", Band::B14, Mode::Cw, "100"),
            q("JA2AAA", Band::B14, Mode::Cw, "KW"),
            q("IT9AAA", Band::B14, Mode::Cw, "100"), // no WAE list: Italy
            q("KH6AA", Band::B14, Mode::Cw, "100"),  // Hawaii is DX
            q("VE3AA", Band::B14, Mode::Cw, "ON"),   // rules 5.2.2: no multiplier
        ],
    );
    let (_, s) = run(ArrlDxW, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (5, 15, 3, 45));
    assert_eq!(l.qsos[2].multi1, "I");
}

#[test]
fn waedc() {
    // UWAEMulti.pas (Europe only), UWAEScore.pas: (QSOs + QTCs) x factor x multis.
    let mut tagged = q("DL1AA", Band::B7, Mode::Cw, "001");
    tagged.memo = "[QTC 10]".into();
    let mut l = log(
        "JA1ZLO",
        vec![
            q("DL1AA", Band::B3_5, Mode::Cw, "001"), // 80 m factor 4
            tagged,                                  // 40 m factor 3, one QTC
            q("DJ1BB", Band::B7, Mode::Cw, "002"),
            q("W1AW", Band::B7, Mode::Cw, "003"),   // not Europe
            q("IT9AA", Band::B14, Mode::Cw, "004"), // *IT9, factor 2
        ],
    );
    let (_, s) = run(Wae, ContestMode::Cw, &mut l);
    // points 4, QTCs 1; multis 4 + 3 + 2 = 9.
    assert_eq!(totals(&s), (5, 4, 9, 45));
    assert_eq!(l.qsos[3].multi1, "Non-EU");
}

#[test]
fn iota() {
    // UIOTAMulti.pas AddNoUpdate: island 15, no island or own island 3; islands
    // by band and mode.
    let mut first = q("JA2AAA", Band::B14, Mode::Cw, "001AS007");
    first.nr_sent = "001AS007".into();
    let mut l = log(
        "JA1ZLO",
        vec![
            first,
            q("DL1AA", Band::B14, Mode::Cw, "005"),
            q("G4AAA", Band::B14, Mode::Cw, "010EU005"),
            q("G4AAA", Band::B14, Mode::Ssb, "011EU005"),
        ],
    );
    let (c, s) = run(Iota, ContestMode::Mix, &mut l);
    // IOTA rules 2023/2024, island side: own reference 5, world 5, island 15:
    // 5 + 5 + 15 + 15; AS007, EU005 CW, EU005 SSB.
    assert_eq!(totals(&s), (4, 40, 3, 120));
    assert!(matches!(
        c.check_multi(&q("X", Band::B21, Mode::Cw, "EU005")),
        MultiCheck::Known { needed: true, .. }
    ));
}

#[test]
fn arrl10() {
    // UARRL10Multi.pas (mode slots), UARRL10Score.pas (CW 4, /N 8, phone 2).
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B28, Mode::Cw, "CT"),
            q("W1AW", Band::B28, Mode::Ssb, "CT"),
            q("K1AA/N", Band::B28, Mode::Cw, "MA"),
            q("DL1AA", Band::B28, Mode::Cw, "123"),
            q("XE1AA", Band::B28, Mode::Ssb, "TLAX"),
        ],
    );
    let (c, s) = run(Arrl10, ContestMode::Mix, &mut l);
    // ARRL 10 m rules 2026 v2.0 5.1: no novice bonus (zLog gives /N 8 points):
    // 4 + 2 + 4 + 4 + 2 = 16; CT cw, CT ph, MA cw, DL cw, TLX ph.
    assert_eq!(totals(&s), (5, 16, 5, 80));
    assert!(c.valid_number(&q("X/MM", Band::B28, Mode::Cw, "2")));
    assert!(!c.valid_number(&q("DL1AA", Band::B28, Mode::Cw, "CT")));
    let me = Station {
        callsign: "W1AW".into(),
        prov: "CT".into(),
        ..Station::default()
    };
    assert_eq!(c.sent_number(Band::B28, Power::P100, &me), "CT");
}

#[test]
fn all_asian() {
    // UARRLWMulti.pas AddNoUpdate with ALLASIANFLAG.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("JA2AAA", Band::B14, Mode::Cw, "50"),
            q("BY1AA", Band::B1_9, Mode::Cw, "40"),
            q("BY1AA", Band::B28, Mode::Cw, "40"),
            q("DL1AA", Band::B7, Mode::Cw, "30"),
            q("W1AW", Band::B3_5, Mode::Cw, "30"),
        ],
    );
    let (_, s) = run(AllAsian, ContestMode::Cw, &mut l);
    // 0 + 3 + 2 + 3 + 6 = 14; BY, BY, DL, K (own country JA: no multiplier, per
    // zLog and the JARL rules).
    assert_eq!(totals(&s), (5, 14, 4, 56));
    assert!(!l.qsos[0].new_multi1);
}

#[test]
fn jidx() {
    // UJIDXMulti.pas (zones, countries), UJIDXScore2.pas (points by band).
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B1_9, Mode::Cw, "5"),
            q("K6XX", Band::B1_9, Mode::Cw, "3"),
            q("DL1AA", Band::B7, Mode::Cw, "14"),
            q("JA2AAA", Band::B7, Mode::Cw, "25"), // JIDX rules 2026: JA-JA scores nothing
        ],
    );
    let (_, s) = run(Jidx, ContestMode::Cw, &mut l);
    // 4 + 4 + 1 = 9; zones 5, 3, 14; K, DL.
    assert_eq!(totals(&s), (4, 9, 5, 45));
    // UJIDX_DX_Multi.pas: prefectures 1 to 50 per band.
    let mut l = log(
        "W1AW",
        vec![
            q("JA1ZLO", Band::B28, Mode::Cw, "10"),
            q("JA1BBB", Band::B28, Mode::Cw, "10"),
            q("JD1AA", Band::B14, Mode::Cw, "48"),
        ],
    );
    let (c, s) = run(JidxDx, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (3, 5, 2, 10));
    assert!(!c.valid_number(&q("JA1ZLO", Band::B14, Mode::Cw, "51")));
    assert_eq!(c.multi_table()[47].1, "Ogasawara");
}

#[test]
fn ap_sprint() {
    // UAPSprintScore.pas: 1 point; UWPXMulti.pas prefixes once per contest.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("JA2AAA", Band::B7, Mode::Cw, "001"),
            q("JA2AAA", Band::B14, Mode::Cw, "002"),
            q("VK2AA", Band::B21, Mode::Cw, "003"),
        ],
    );
    let (_, s) = run(ApSprint, ContestMode::Cw, &mut l);
    // AP Sprint rules 2026: 20 and 15 m only, so the 40 m QSO is out of the contest.
    assert_eq!(totals(&s), (2, 2, 2, 4));
}

#[test]
fn bartg() {
    // UJarlWorldWideRTTYMulti.pas AddNoUpdate (BARTG rule): countries and call
    // areas per band, continents once; 1 point, /MM 2 points.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Rtty, "001"),  // K, K1, NA
            q("K1AR", Band::B14, Mode::Rtty, "002"),  // nothing new
            q("W6AA", Band::B14, Mode::Rtty, "003"),  // K6
            q("DL1AA", Band::B21, Mode::Rtty, "004"), // DL, EU
            q("DL1AA/MM", Band::B7, Mode::Rtty, "005"),
        ],
    );
    let (_, s) = run(Bartg, ContestMode::Rtty, &mut l);
    // BARTG rules 2025 v3: 1 point each (no /MM bonus); score = points x
    // (K, K1, K6, DL) x continents (NA, EU) = 5 x 4 x 2.
    assert_eq!(totals(&s), (5, 5, 4, 40));
    assert_eq!(l.qsos[0].multi1, "K,K1");
}

#[test]
fn pedi() {
    // UPediScore.pas: one point per QSO, every mode, no multipliers.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B10, Mode::Cw, ""),
            q("W1AW", Band::B10, Mode::Ft8, ""),
            q("W1AW", Band::B144, Mode::Fm, ""),
        ],
    );
    let (_, s) = run(Pedi, ContestMode::All, &mut l);
    assert_eq!(totals(&s), (3, 3, 0, 3));
}

#[test]
fn without_cty_everything_is_unknown() {
    // GetPrefix returns the unknown country; AnalyzeMyCountry keeps JA/AS.
    let mut c = WorldRules::new(Cqww, ContestMode::Cw, Arc::new(Cty::default()));
    let mut l = log("JA1ZLO", vec![q("W1AW", Band::B14, Mode::Cw, "5")]);
    let s = rescore(&mut l, &mut c).unwrap();
    assert_eq!(totals(&s), (1, 3, 1, 3));
}

#[test]
fn jarl_rtty() {
    // UJarlWorldWideRTTYMulti.pas AddNoUpdate (JARL rule) and the JARL rules page
    // (2026): 2 points same continent, 3 other, /MM 2 without multipliers;
    // entities except JA/W/VE/VK per band, call areas of those per band.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("JA2AAA", Band::B14, Mode::Rtty, "40"),  // AS: 2, JA2
            q("JA2BBB", Band::B14, Mode::Rtty, "30"),  // 2, nothing new
            q("W1AW", Band::B14, Mode::Rtty, "50"),    // NA: 3, K1
            q("BY1AA", Band::B14, Mode::Rtty, "20"),   // 2, BY
            q("DL1AA", Band::B21, Mode::Rtty, "60"),   // EU: 3, DL
            q("DL2AA/MM", Band::B7, Mode::Rtty, "70"), // 2, no multiplier
            q("W1AW", Band::B14, Mode::Rtty, "50"),    // dupe: 0
            q("JA3AAA", Band::B14, Mode::Cw, "40"),    // not RTTY: 0
        ],
    );
    let (_, s) = run(JarlRtty, ContestMode::Rtty, &mut l);
    // (2 + 2 + 3 + 2 + 3 + 2) x (JA2, K1, BY, DL).
    assert_eq!((s.total.points, s.total.multi, s.score), (14, 4, 56));
    assert_eq!(l.qsos[0].multi1, "JA2");
    assert_eq!(l.qsos[2].multi1, "K1");
    assert!(l.qsos[5].multi1.is_empty());
}

fn station_aged(age: &str) -> Station {
    Station {
        callsign: "JA1ZLO".into(),
        age: age.into(),
        ..Station::default()
    }
}

fn invalid(mut x: Qso) -> Qso {
    x.invalid = true;
    x
}

#[test]
fn jarl_rtty_unknown_call_scores_3_without_multi() {
    // UzLogGlobal.pas GetPrefix returns the unknown entry for a call cty.dat does not
    // know; its empty continent is not ours, so 3 points (JARL rule).
    let mut l = log(
        "JA1ZLO",
        vec![
            q("ZZ9ZZ", Band::B14, Mode::Rtty, "40"),
            q("ZZ8ZZ", Band::B14, Mode::Rtty, "40"),
            q("ZZ9ZZ", Band::B14, Mode::Rtty, "40"), // dupe: 0
        ],
    );
    let (_, s) = run(JarlRtty, ContestMode::Rtty, &mut l);
    assert_eq!((s.total.points, s.total.multi, s.score), (6, 0, 0));
    assert_eq!(l.qsos[0].points, 3);
    assert!(l.qsos[0].multi1.is_empty() && !l.qsos[0].new_multi1);
    assert_eq!(l.qsos[2].points, 0);
}

#[test]
fn jarl_rtty_mm_scores_2_and_marks_nothing() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("DL1AA/MM", Band::B14, Mode::Rtty, "40"),
            q("DL2AA", Band::B14, Mode::Rtty, "40"), // DL still new on 14
            q("ZZ9ZZ/MM", Band::B14, Mode::Rtty, "40"),
        ],
    );
    let (_, s) = run(JarlRtty, ContestMode::Rtty, &mut l);
    assert_eq!((s.total.points, s.total.multi), (2 + 3 + 2, 1));
    assert!(l.qsos[1].new_multi1);
}

#[test]
fn jarl_rtty_bands_and_vk0() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B1_9, Mode::Rtty, "40"), // below 3.5 MHz: out
            q("W1AW", Band::B10, Mode::Rtty, "40"),  // WARC: out
            q("W1AW", Band::B28, Mode::Rtty, "40"),  // K1 on 28
            q("W1AW", Band::B3_5, Mode::Rtty, "40"), // K1 on 3.5
            q("VK0AA", Band::B14, Mode::Rtty, "40"), // VK0: no call area of the list
            q("W1AW", Band::B50, Mode::Rtty, "40"),  // out
        ],
    );
    let (_, s) = run(JarlRtty, ContestMode::Rtty, &mut l);
    assert_eq!(totals(&s), (3, 3 + 3 + 3, 2, 18));
    assert!(l.qsos[0].multi1.is_empty() && l.qsos[0].points == 0);
    assert!(l.qsos[4].multi1.is_empty());
}

#[test]
fn jarl_rtty_and_all_asian_send_the_age() {
    // FSentStr := '$A' (UzLogContest.pas:2583, 2650).
    let st = station_aged("45");
    for (kind, mode) in [(JarlRtty, ContestMode::Rtty), (AllAsian, ContestMode::Cw)] {
        let c = rules(kind, mode);
        assert_eq!(c.sent_number(Band::B14, Power::P100, &st), "45");
        assert_eq!(
            c.sent_number(Band::B14, Power::P100, &Station::default()),
            ""
        );
    }
}

#[test]
fn jarl_rtty_multi_table_has_call_areas() {
    // UJarlWorldWideRTTYMulti.pas FormCreate: JA, K, VE, VK replaced in place by
    // JA0-9, K0-9, VE0-9, VK1-8.
    let mut l = log("JA1ZLO", vec![q("JA2AAA", Band::B14, Mode::Rtty, "40")]);
    let (c, _) = run(JarlRtty, ContestMode::Rtty, &mut l);
    let t = c.multi_table();
    let keys: Vec<&str> = t.iter().map(|r| r.0.as_str()).collect();
    for gone in ["JA", "K", "VE", "VK", "VK0", "VK9"] {
        assert!(!keys.contains(&gone), "{gone}");
    }
    let areas = keys
        .iter()
        .filter(|k| {
            ["JA", "K", "VE", "VK"]
                .iter()
                .any(|c| k.strip_prefix(c).is_some_and(|d| d.len() == 1))
        })
        .count();
    assert_eq!(areas, 38);
    // In place: Japan's areas come first, as Japan is first in the fixture.
    assert_eq!(&keys[..2], &["JA0", "JA1"]);
    let ja2 = t.iter().find(|r| r.0 == "JA2").unwrap();
    assert_eq!(ja2.1, "Japan (Area 2)");
    assert_eq!(ja2.2, vec![Band::B14]);
    assert!(keys.contains(&"DL"));
}

#[test]
fn bartg_multi_table_keeps_countries_and_appends_call_areas() {
    let mut l = log("JA1ZLO", vec![q("W6AA", Band::B14, Mode::Rtty, "001")]);
    let (c, _) = run(Bartg, ContestMode::Rtty, &mut l);
    let t = c.multi_table();
    let keys: Vec<&str> = t.iter().map(|r| r.0.as_str()).collect();
    assert!(keys.contains(&"K") && keys.contains(&"JA"));
    let k0 = keys.iter().position(|k| *k == "K0").unwrap();
    assert_eq!(&keys[k0..k0 + 2], &["K0", "K1"]);
    assert_eq!(keys.last(), Some(&"VK8"));
    let k6 = t.iter().find(|r| r.0 == "K6").unwrap();
    assert_eq!(k6.2, vec![Band::B14]);
}

#[test]
fn bartg_bands_mode_and_dupes() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("K1AR", Band::B14, Mode::Cw, "001"),   // not RTTY: no score
            q("W1AW", Band::B10, Mode::Rtty, "002"), // WARC: out
            q("W1AW", Band::B14, Mode::Rtty, "003"), // K, K1, NA
            q("W1AW", Band::B14, Mode::Rtty, "004"), // dupe
            q("W1AW", Band::B21, Mode::Rtty, "005"), // K, K1 on 21
        ],
    );
    let (_, s) = run(Bartg, ContestMode::Rtty, &mut l);
    // 2 points x 4 multipliers x 1 continent.
    assert_eq!((s.total.points, s.total.multi, s.score), (2, 4, 8));
    assert!(l.qsos[3].dupe && l.qsos[3].points == 0);
}

#[test]
fn periods_and_start_hours() {
    // FPeriod and FStartTime of each constructor in UzLogContest.pas.
    let expect = [
        (Cqww, 48, Some(0)),
        (Wpx, 48, Some(0)),
        (Iaru, 24, Some(12)),
        (ArrlDxDx, 48, Some(0)),
        (ArrlDxW, 48, Some(0)),
        (Wae, 48, Some(0)),
        (Iota, 24, Some(12)),
        (Arrl10, 48, Some(0)),
        (AllAsian, 48, Some(0)),
        (Jidx, 30, Some(7)),
        (JidxDx, 30, Some(7)),
        (ApSprint, 0, None),
        (Bartg, 48, Some(0)),
        (JarlRtty, 48, Some(0)),
        (Pedi, 0, None),
    ];
    assert_eq!(expect.len(), WorldContest::ALL.len());
    for (kind, hours, start) in expect {
        let c = rules(kind, ContestMode::Cw);
        assert_eq!(c.period_hours(), hours, "{kind:?}");
        assert_eq!(c.start_hour(), start, "{kind:?}");
    }
}

fn adif(kind: WorldContest, mode: ContestMode, x: &Qso) -> Vec<(String, String)> {
    rules(kind, mode).adif_fields(x)
}

fn field(v: &[(String, String)], name: &str) -> Option<String> {
    v.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone())
}

#[test]
fn adif_fields_of_jarl_rtty() {
    // AdifContestId 'JARL-WW-RTTY' in every mode; ADIF_ExchangeRX_FieldName 'age'.
    let x = q("W1AW", Band::B14, Mode::Rtty, "52");
    for mode in [ContestMode::Rtty, ContestMode::Mix] {
        let v = adif(JarlRtty, mode, &x);
        assert_eq!(field(&v, "contest_id").as_deref(), Some("JARL-WW-RTTY"));
        assert_eq!(field(&v, "age").as_deref(), Some("52"));
    }
}

#[test]
fn adif_fields_follow_the_mode() {
    let x = q("W1AW", Band::B14, Mode::Cw, "5");
    let id = |k, m| field(&adif(k, m, &x), "contest_id").unwrap();
    assert_eq!(id(Cqww, ContestMode::Cw), "CQWW-CW");
    assert_eq!(id(Cqww, ContestMode::Phone), "CQWW-SSB");
    assert_eq!(id(Cqww, ContestMode::Rtty), "CQWW-RTTY");
    assert_eq!(id(Cqww, ContestMode::Mix), "");
    assert_eq!(id(Wpx, ContestMode::Phone), "CQWPX-SSB");
    assert_eq!(id(Wae, ContestMode::Cw), "DARC-WAEDC-CW");
    assert_eq!(id(ArrlDxDx, ContestMode::Phone), "ARRLDX-SSB");
    assert_eq!(id(ArrlDxW, ContestMode::Rtty), "");
    assert_eq!(id(AllAsian, ContestMode::Cw), "ALL-ASIAN-DX-CW");
    assert_eq!(id(Jidx, ContestMode::Phone), "JIDX-SSB");
    assert_eq!(id(JidxDx, ContestMode::Rtty), "");
    assert_eq!(id(Iaru, ContestMode::Mix), "IARU-HF");
    assert_eq!(id(Iota, ContestMode::Cw), "RSGB-IOTA");
    assert_eq!(id(Arrl10, ContestMode::Mix), "ARRL-10");
    assert_eq!(id(ApSprint, ContestMode::Cw), "AP-SPRINT");
    assert_eq!(id(Bartg, ContestMode::Rtty), "BARTG-RTTY");
    assert_eq!(id(Pedi, ContestMode::All), "");
}

#[test]
fn adif_exchange_and_extra_fields() {
    let x = q("W1AW", Band::B14, Mode::Cw, "5");
    let f = |k, n| field(&adif(k, ContestMode::Cw, &x), n);
    assert_eq!(f(Cqww, "cqz").as_deref(), Some("5"));
    assert_eq!(f(Iaru, "ituz").as_deref(), Some("5"));
    assert_eq!(f(ArrlDxDx, "state").as_deref(), Some("5"));
    assert_eq!(f(ArrlDxW, "rx_pwr").as_deref(), Some("5"));
    assert_eq!(f(AllAsian, "age").as_deref(), Some("5"));
    assert_eq!(f(Bartg, "cont").as_deref(), Some("NA"));
    assert_eq!(adif(Wae, ContestMode::Cw, &x).len(), 1);
    let mut scored = x.clone();
    scored.multi1 = "W1".into();
    let v = adif(Wpx, ContestMode::Cw, &scored);
    assert_eq!(field(&v, "pfx").as_deref(), Some("W1"));
    // An unknown call has no continent.
    let u = q("ZZ9ZZ", Band::B14, Mode::Rtty, "001");
    assert_eq!(
        field(&adif(Bartg, ContestMode::Rtty, &u), "cont").as_deref(),
        Some("")
    );
}

#[test]
fn cqww_band_edges_and_invalid() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B1_9, Mode::Cw, "5"),           // 3, zone 5 + K
            q("W1AW", Band::B28, Mode::Cw, "5"),            // 3, zone 5 + K on 28
            q("W1AW", Band::B10, Mode::Cw, "5"),            // WARC: out
            q("W1AW", Band::B50, Mode::Cw, "5"),            // above 28 MHz: out
            invalid(q("DL1AA", Band::B14, Mode::Cw, "14")), // invalid: out
            q("DL1AA", Band::B14, Mode::Cw, "41"),          // 3, zone 41 is no multiplier
        ],
    );
    let (c, s) = run(Cqww, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (3, 9, 5, 45));
    assert!(!s.bands.contains_key(&Band::B10));
    assert!(!l.qsos[4].dupe && l.qsos[4].points == 0);
    assert!(!c.valid_number(&q("DL1AA", Band::B14, Mode::Cw, "41")));
    assert!(!c.valid_number(&q("DL1AA", Band::B14, Mode::Cw, "")));
}

#[test]
fn wpx_prefix_once_and_band_edges() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Cw, "001"),  // 3, W1
            q("W1BB", Band::B21, Mode::Cw, "002"),  // 3, W1 not new
            q("W1CC", Band::B7, Mode::Cw, "003"),   // 6
            q("W1AW", Band::B14, Mode::Cw, "004"),  // dupe
            q("W1DD", Band::B18, Mode::Cw, "005"),  // WARC: out
            q("W1EE", Band::B14, Mode::Ssb, "006"), // phone in a CW log
        ],
    );
    let (c, s) = run(Wpx, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (4, 12, 1, 12));
    assert!(l.qsos[3].dupe);
    assert!(c.valid_number(&q("X", Band::B14, Mode::Cw, "0")));
    assert!(!c.valid_number(&q("X", Band::B14, Mode::Cw, "5A")));
    assert!(!c.valid_number(&q("X", Band::B14, Mode::Cw, "")));
}

#[test]
fn wpx_rtty_points() {
    // UWPXMulti.pas RTTY: other continent 3, same continent 2, own country 1;
    // doubled on 80 and 40 m.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B3_5, Mode::Rtty, "001"),  // 6
            q("BY1AA", Band::B14, Mode::Rtty, "002"),  // 2
            q("JA2AAA", Band::B7, Mode::Rtty, "003"),  // 2
            q("JA3AAA", Band::B28, Mode::Rtty, "004"), // 1
        ],
    );
    let (_, s) = run(Wpx, ContestMode::Rtty, &mut l);
    assert_eq!(totals(&s), (4, 11, 4, 44));
}

#[test]
fn iaru_band_edges_and_invalid() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B1_9, Mode::Cw, "8"),         // 5
            q("W1AW", Band::B24, Mode::Cw, "8"),          // WARC: out
            invalid(q("W1AW", Band::B14, Mode::Cw, "8")), // invalid
            q("W1AW", Band::B14, Mode::Cw, "8"),          // 5, 8 on 14
            q("W1AW", Band::B14, Mode::Cw, "8"),          // dupe
        ],
    );
    let (c, s) = run(Iaru, ContestMode::Mix, &mut l);
    assert_eq!(totals(&s), (3, 10, 2, 20));
    assert!(l.qsos[4].dupe);
    assert!(!c.valid_number(&q("W1AW", Band::B14, Mode::Cw, "")));
}

#[test]
fn iaru_mode_category() {
    // A CW entry does not score phone QSOs.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Cw, "8"),
            q("W1AW", Band::B14, Mode::Ssb, "8"),
        ],
    );
    let (_, s) = run(Iaru, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (1, 5, 1, 5));
}

#[test]
fn arrl_dx_dx_dupes_and_band_edges() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B1_9, Mode::Cw, "CT"),
            q("W1AW", Band::B1_9, Mode::Cw, "CT"), // dupe
            q("W1AW", Band::B10, Mode::Cw, "CT"),  // WARC: out
            q("W1AW", Band::B50, Mode::Cw, "CT"),  // out
            q("K1AR", Band::B28, Mode::Cw, "MA"),
        ],
    );
    let (_, s) = run(ArrlDxDx, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (3, 6, 2, 12));
}

#[test]
fn arrl_dx_dx_mode_and_unknown_state() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Ssb, "CT"), // phone in a CW log
            q("K1AR", Band::B14, Mode::Cw, "ZZ"),  // 3 points, no state
            q("W1AW", Band::B14, Mode::Cw, "CT"),  // dupe of the phone QSO
        ],
    );
    let (c, s) = run(ArrlDxDx, ContestMode::Cw, &mut l);
    // Dupes ignore the mode (`AcceptDifferentMode` off).
    assert_eq!(totals(&s), (2, 3, 0, 0));
    assert!(l.qsos[2].dupe);
    assert!(c.valid_number(&q("W1AW", Band::B14, Mode::Cw, "CT")));
}

#[test]
fn arrl_dx_w_countries_per_band() {
    let mut l = log(
        "W1AW",
        vec![
            q("DL1AA", Band::B14, Mode::Cw, "100"),
            q("DL2AA", Band::B14, Mode::Cw, "100"), // DL not new
            q("DL1AA", Band::B21, Mode::Cw, "100"), // DL on 21
            q("DL1AA", Band::B21, Mode::Cw, "100"), // dupe
            q("XE1AA", Band::B18, Mode::Cw, "100"), // WARC: out
        ],
    );
    let (c, s) = run(ArrlDxW, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (4, 9, 2, 18));
    assert!(!c.valid_number(&q("DL1AA", Band::B14, Mode::Cw, "")));
}

#[test]
fn arrl_dx_w_mode_category() {
    let mut l = log(
        "W1AW",
        vec![
            q("DL1AA", Band::B14, Mode::Cw, "100"),
            q("JA1ZLO", Band::B14, Mode::Ssb, "100"),
        ],
    );
    let (_, s) = run(ArrlDxW, ContestMode::Phone, &mut l);
    assert_eq!(totals(&s), (1, 3, 1, 3));
}

#[test]
fn waedc_band_edges_and_dupes() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("DL1AA", Band::B1_9, Mode::Cw, "001"), // below 80 m: out
            q("DL1AA", Band::B28, Mode::Cw, "002"),  // factor 2
            q("DL1AA", Band::B28, Mode::Cw, "003"),  // dupe
            q("DL1AA", Band::B10, Mode::Cw, "004"),  // WARC: out
        ],
    );
    let (_, s) = run(Wae, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (2, 1, 2, 2));
}

#[test]
fn waedc_rtty_non_europe_is_a_multiplier() {
    let mut tagged = q("DL1AA", Band::B14, Mode::Rtty, "002");
    tagged.memo = "[QTC 5]".into();
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Rtty, "001"), // 1, K factor 2
            tagged,                                  // 1, DL factor 2, a QTC
        ],
    );
    let (_, s) = run(Wae, ContestMode::Rtty, &mut l);
    // WAE rules 2025 section 12: (2 points + 1 QTC) x 4.
    assert_eq!(totals(&s), (2, 2, 4, 12));
    assert_eq!(l.qsos[0].multi1, "K");
}

#[test]
fn iota_world_station_points() {
    // IOTA rules: world station working world 2, an island 15.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("DL1AA", Band::B14, Mode::Cw, "001"),
            q("G4AAA", Band::B14, Mode::Cw, "002EU005"),
            q("G4AAA", Band::B21, Mode::Cw, "003EU005"), // EU005 new on 21
            q("G4AAA", Band::B21, Mode::Cw, "004EU005"), // dupe
            q("G4AAA", Band::B10, Mode::Cw, "005EU005"), // WARC: out
        ],
    );
    let (_, s) = run(Iota, ContestMode::Mix, &mut l);
    assert_eq!(totals(&s), (4, 32, 2, 64));
    assert!(l.qsos[3].dupe && l.qsos[3].points == 0);
}

#[test]
fn iota_mode_category_and_unknown_reference() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("G4AAA", Band::B14, Mode::Ssb, "001EU005"), // phone in a CW log
            q("G4AAA", Band::B14, Mode::Cw, "001XX999"),  // no island
        ],
    );
    let (c, s) = run(Iota, ContestMode::Cw, &mut l);
    assert_eq!(s.total.multi, 0);
    assert!(c.valid_number(&q("X", Band::B14, Mode::Cw, "")));
}

#[test]
fn arrl10_only_28_mhz_and_dupes() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B21, Mode::Cw, "CT"), // out
            q("W1AW", Band::B50, Mode::Cw, "CT"), // out
            q("W1AW", Band::B28, Mode::Cw, "CT"), // 4, CT cw
            q("W1AW", Band::B28, Mode::Cw, "CT"), // dupe
            q("K1AR", Band::B28, Mode::Cw, "CT"), // 4, CT cw not new
        ],
    );
    let (_, s) = run(Arrl10, ContestMode::Mix, &mut l);
    assert_eq!(totals(&s), (3, 8, 1, 8));
}

#[test]
fn arrl10_digital_scores_nothing_and_mm_needs_a_state() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("DL1AA", Band::B28, Mode::Ssb, "001"),  // 2, DL ph
            q("DL2AA", Band::B28, Mode::Rtty, "002"), // not in a mixed CW/phone log
            q("W1AW/MM", Band::B28, Mode::Cw, "2"),   // 4, ITU region 2 cw
            q("W2AW/MM", Band::B28, Mode::Cw, "9"),   // 4, no region 9
        ],
    );
    let (_, s) = run(Arrl10, ContestMode::Mix, &mut l);
    assert_eq!((s.total.points, s.total.multi), (10, 2));
    assert!(l.qsos[3].multi1.is_empty());
}

#[test]
fn all_asian_band_edges_and_dupes() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("DL1AA", Band::B28, Mode::Cw, "30"), // 6
            q("DL1AA", Band::B28, Mode::Cw, "30"), // dupe
            q("DL1AA", Band::B10, Mode::Cw, "30"), // WARC: out
            q("DL1AA", Band::B50, Mode::Cw, "30"), // out
            q("DL1AA", Band::B21, Mode::Cw, "30"), // 3, DL on 21
        ],
    );
    let (c, s) = run(AllAsian, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (3, 9, 2, 18));
    assert!(!c.valid_number(&q("DL1AA", Band::B14, Mode::Cw, "")));
}

#[test]
fn all_asian_phone_entry() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("BY1AA", Band::B14, Mode::Ssb, "40"), // 1
            q("BY2AA", Band::B14, Mode::Cw, "40"),  // CW in a phone log: no score
        ],
    );
    let (_, s) = run(AllAsian, ContestMode::Phone, &mut l);
    assert_eq!((s.total.points, s.total.multi, s.score), (1, 1, 1));
}

#[test]
fn jidx_band_edges_and_dupes() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B28, Mode::Cw, "5"),  // 2, zone 5 + K
            q("W1AW", Band::B28, Mode::Cw, "5"),  // dupe
            q("W1AW", Band::B10, Mode::Cw, "5"),  // WARC: out
            q("W1AW", Band::B50, Mode::Cw, "5"),  // out
            q("W1AW", Band::B14, Mode::Cw, "41"), // 1, K on 14, no zone
        ],
    );
    let (_, s) = run(Jidx, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (3, 3, 3, 9));
}

#[test]
fn jidx_dx_band_edges_and_non_japan() {
    let mut l = log(
        "W1AW",
        vec![
            q("JA1ZLO", Band::B1_9, Mode::Cw, "10"), // 4
            q("JA1ZLO", Band::B1_9, Mode::Cw, "10"), // dupe
            q("DL1AA", Band::B14, Mode::Cw, "10"),   // not Japan: nothing
            q("JA1ZLO", Band::B10, Mode::Cw, "10"),  // WARC: out
            q("JA1ZLO", Band::B21, Mode::Cw, "51"),  // 1, no prefecture
        ],
    );
    let (_, s) = run(JidxDx, ContestMode::Cw, &mut l);
    assert_eq!((s.total.points, s.total.multi, s.score), (5, 1, 5));
}

#[test]
fn ap_sprint_prefix_once_and_dupes() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("VK2AA", Band::B14, Mode::Cw, "001"),
            q("VK2AA", Band::B14, Mode::Cw, "002"), // dupe
            q("VK2BB", Band::B21, Mode::Cw, "003"), // VK2 not new
            q("VK2AA", Band::B28, Mode::Cw, "004"), // out
        ],
    );
    let (c, s) = run(ApSprint, ContestMode::Cw, &mut l);
    assert_eq!(totals(&s), (3, 2, 1, 2));
    assert!(!c.valid_number(&q("X", Band::B14, Mode::Cw, "A")));
}

#[test]
fn ap_sprint_mode_category() {
    let mut l = log(
        "JA1ZLO",
        vec![
            q("VK2AA", Band::B14, Mode::Ssb, "001"),
            q("VK3AA", Band::B14, Mode::Cw, "002"),
        ],
    );
    let (_, s) = run(ApSprint, ContestMode::Phone, &mut l);
    assert_eq!((s.total.points, s.total.multi), (1, 1));
}

#[test]
fn pedi_counts_every_band_and_dupes_score() {
    // UPediScore.pas: every QSO one point, dupes included; no period.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B1_9, Mode::Cw, ""),
            q("W1AW", Band::B1_9, Mode::Cw, ""),
            q("W1AW", Band::B248G, Mode::Fm, ""),
        ],
    );
    let (c, s) = run(Pedi, ContestMode::All, &mut l);
    assert_eq!(s.total.points, 3);
    assert!(c.valid_number(&q("X", Band::B14, Mode::Cw, "")));
    assert!(c.multi_table().is_empty());
}

#[test]
fn pedi_invalid_qso_scores_nothing() {
    let mut l = log(
        "JA1ZLO",
        vec![
            invalid(q("W1AW", Band::B14, Mode::Cw, "")),
            q("W1AW", Band::B14, Mode::Cw, ""),
        ],
    );
    let (_, s) = run(Pedi, ContestMode::All, &mut l);
    assert_eq!(totals(&s), (1, 1, 0, 1));
}

#[test]
fn jidx_period_is_30_hours() {
    // jidx.org rules: 07:00Z Saturday to 13:00Z Sunday.
    for kind in [Jidx, JidxDx] {
        let c = rules(kind, ContestMode::Cw);
        assert_eq!(c.period_hours(), 30);
        assert_eq!(c.start_hour(), Some(7));
    }
}

#[test]
fn wae_rtty_counts_non_eu_entities() {
    // WAE rules 2025 section 12: every entity is a multiplier in RTTY.
    let mut l = log(
        "JA1ZLO",
        vec![
            q("W1AW", Band::B14, Mode::Rtty, "001"),
            q("VE3AA", Band::B14, Mode::Rtty, "002"),
            q("K1AR", Band::B14, Mode::Rtty, "003"), // K again
            q("W1AW", Band::B14, Mode::Rtty, "004"), // dupe
            q("DL1AA", Band::B14, Mode::Rtty, "005"),
        ],
    );
    let (_, s) = run(Wae, ContestMode::Rtty, &mut l);
    assert_eq!(l.qsos[0].multi1, "K");
    assert!(l.qsos[0].new_multi1 && l.qsos[1].new_multi1);
    assert!(!l.qsos[2].new_multi1);
    assert_eq!(l.qsos[3].points, 0);
    assert!(l.qsos[4].new_multi1);
    assert_eq!(s.total.points, 4);
}

#[test]
fn wae_cw_still_ignores_non_eu() {
    let mut l = log("JA1ZLO", vec![q("W1AW", Band::B14, Mode::Cw, "001")]);
    let (_, s) = run(Wae, ContestMode::Cw, &mut l);
    assert_eq!(l.qsos[0].multi1, "Non-EU");
    assert_eq!(totals(&s), (1, 0, 0, 0));
}

#[test]
fn arrl10_sent_state_or_province() {
    let c = rules(Arrl10, ContestMode::Mix);
    let st = |call: &str| Station {
        callsign: call.into(),
        prov: "ON".into(),
        ..Station::default()
    };
    assert_eq!(c.sent_number(Band::B28, Power::P100, &st("VE3AA")), "ON");
    assert_eq!(c.sent_number(Band::B28, Power::P100, &st("XE1AA")), "ON");
    assert_eq!(c.sent_number(Band::B28, Power::P100, &st("W1AW")), "ON");
    // /MM sends the ITU region of the station settings, empty when not set.
    assert_eq!(c.sent_number(Band::B28, Power::P100, &st("W1AW/MM")), "");
    let mm = Station {
        itu_region: "3".into(),
        ..st("JA1ZLO/MM")
    };
    assert_eq!(c.sent_number(Band::B28, Power::P100, &mm), "3");
    assert_eq!(c.sent_number(Band::B28, Power::P100, &st("JA1ZLO")), "001");
    assert_eq!(c.sent_number(Band::B28, Power::P100, &st("")), "001");
}

fn on_tx(mut x: Qso, tx: u8, serial: i32) -> Qso {
    x.tx = tx;
    x.serial = serial;
    x
}

#[test]
fn serials_follow_the_own_tx() {
    // TLog.GetLastSerial (stAll): the last QSO with Q.TX = txnr (UzLogQSO.pas).
    let mut l = log(
        "JA1ZLO",
        vec![
            on_tx(q("W1AW", Band::B7, Mode::Cw, "001"), 0, 1),
            on_tx(q("K6XX", Band::B7, Mode::Cw, "001"), 1, 1),
            on_tx(q("DL1AA", Band::B14, Mode::Cw, "002"), 1, 2),
            on_tx(q("BY1AA", Band::B14, Mode::Cw, "003"), 0, 2),
        ],
    );
    let (c, _) = run(Wpx, ContestMode::Cw, &mut l);
    assert_eq!(c.next_serial_of_tx(Band::B7, 0), Some(3));
    assert_eq!(c.next_serial_of_tx(Band::B7, 1), Some(3));
    assert_eq!(c.next_serial_of_tx(Band::B7, 5), Some(1));
    assert_eq!(c.next_serial_on(Band::B7), Some(3));
    let st = Station {
        tx_nr: 5,
        ..Station::default()
    };
    assert_eq!(c.sent_number(Band::B7, Power::P100, &st), "001");
    // Contests without serials have none for any TX.
    let (c, _) = run(Cqww, ContestMode::Cw, &mut l);
    assert_eq!(c.next_serial_of_tx(Band::B7, 1), None);
}

#[test]
fn serial_of_an_empty_log_and_of_unnumbered_qsos() {
    let c = rules(Wae, ContestMode::Cw);
    assert_eq!(c.next_serial_on(Band::B7), Some(1));
    // A QSO without a stored serial falls back to the digits sent (departure: zLog
    // reads TQSO.Serial only), and to 0 without digits.
    let mut l = log("JA1ZLO", vec![q("DL1AA", Band::B7, Mode::Cw, "")]);
    l.qsos[0].nr_sent = "012".into();
    let (c, _) = run(Wae, ContestMode::Cw, &mut l);
    assert_eq!(c.next_serial_on(Band::B7), Some(13));
    l.qsos[0].nr_sent = "ABC".into();
    let (c, _) = run(Wae, ContestMode::Cw, &mut l);
    assert_eq!(c.next_serial_on(Band::B7), Some(1));
}

#[test]
fn serial_after_a_number_sent_with_the_time() {
    // BARTG sends `$S$D`: serial 001 and 13:07 UTC; the next serial is 2.
    let mut l = log("JA1ZLO", vec![q("G3AAA", Band::B14, Mode::Rtty, "")]);
    l.qsos[0].nr_sent = "0011307".into();
    let (c, _) = run(Bartg, ContestMode::Mix, &mut l);
    assert_eq!(c.next_serial_on(Band::B14), Some(2));
    // Without the time the whole number is the serial.
    let (c, _) = run(Wae, ContestMode::Mix, &mut l);
    assert_eq!(c.next_serial_on(Band::B14), Some(11308));
}

#[test]
fn sent_number_of_every_contest() {
    let me = Station {
        callsign: "JA1ZLO".into(),
        prov: "10".into(),
        age: "45".into(),
        ..Station::default()
    };
    let sent = |k| rules(k, ContestMode::Mix).sent_number(Band::B14, Power::P100, &me);
    assert_eq!(sent(Cqww), "25");
    assert_eq!(sent(Iaru), "45");
    assert_eq!(sent(ArrlDxDx), "100");
    assert_eq!(
        rules(ArrlDxDx, ContestMode::Mix).sent_number(Band::B14, Power::P1000, &me),
        "1KW"
    );
    for k in [Wpx, Wae, Iota, ApSprint, Bartg] {
        assert_eq!(sent(k), "001", "{k:?}");
    }
    assert_eq!(sent(AllAsian), "45");
    assert_eq!(sent(JarlRtty), "45");
    assert_eq!(sent(Pedi), "");
    assert_eq!(sent(Arrl10), "001");
    for k in WorldContest::ALL {
        // Every contest answers for an empty station without panicking.
        let _ = rules(k, ContestMode::Mix).sent_number(Band::B14, Power::P100, &Station::default());
    }
}

#[test]
fn exchange_from_past_qsos() {
    let past = |call: &str, nr: &str| q(call, Band::B14, Mode::Cw, nr);
    let c = rules(Cqww, ContestMode::Cw);
    // The CQ zone from CTY.DAT, else the number received.
    assert_eq!(
        c.exchange_from(Band::B7, &past("W1AW", "4")),
        Some("5".into())
    );
    assert_eq!(
        c.exchange_from(Band::B7, &past("K1MM", "")),
        Some("4".into())
    );
    assert_eq!(
        c.exchange_from(Band::B7, &past("ZZ1ZZ", "33")),
        Some("33".into())
    );
    let c = rules(Jidx, ContestMode::Cw);
    assert_eq!(
        c.exchange_from(Band::B7, &past("DL1AA", "")),
        Some("14".into())
    );
    assert_eq!(
        c.exchange_from(Band::B7, &past("ZZ1ZZ", "")),
        Some("".into())
    );
    // Serial contests have nothing to repeat; IOTA repeats a reference only.
    assert_eq!(
        rules(Wpx, ContestMode::Cw).exchange_from(Band::B7, &past("W1AW", "1")),
        None
    );
    let iota = rules(Iota, ContestMode::Cw);
    assert_eq!(iota.exchange_from(Band::B7, &past("W1AW", "001")), None);
    assert_eq!(
        iota.exchange_from(Band::B7, &past("W1AW", "001AS007")),
        Some("001AS007".into())
    );
    // The others repeat the number received.
    let c = rules(ArrlDxDx, ContestMode::Cw);
    assert_eq!(
        c.exchange_from(Band::B7, &past("W1AW", "CT")),
        Some("CT".into())
    );
}

#[test]
fn extract_multi_of_the_arrl_contests() {
    // TARRLDXMulti.ExtractMulti / TARRL10Multi.ExtractMulti: the state or country.
    let dx = rules(ArrlDxDx, ContestMode::Cw);
    assert_eq!(
        dx.extract_multi(&q("W1AW", Band::B14, Mode::Cw, "CT")),
        "CT"
    );
    assert_eq!(dx.extract_multi(&q("W1AW", Band::B14, Mode::Cw, "XX")), "");
    assert_eq!(dx.extract_multi(&q("W1AW", Band::B14, Mode::Cw, "")), "");
    let ten = rules(Arrl10, ContestMode::Mix);
    assert_eq!(
        ten.extract_multi(&q("W1AW", Band::B28, Mode::Cw, "CT")),
        "CT"
    );
    assert_eq!(
        ten.extract_multi(&q("DL1AA", Band::B28, Mode::Cw, "001")),
        "DL"
    );
    assert_eq!(
        ten.extract_multi(&q("ZZ1ZZ", Band::B28, Mode::Cw, "001")),
        ""
    );
    assert_eq!(ten.extract_multi(&q("W1AW", Band::B28, Mode::Cw, "XX")), "");
    // Others: the number received (TBasicMulti.ExtractMulti).
    let ww = rules(Cqww, ContestMode::Cw);
    assert_eq!(ww.extract_multi(&q("W1AW", Band::B14, Mode::Cw, "5")), "5");
}

#[test]
fn check_multi_invalid_paths() {
    let x = |call: &str, nr: &str| q(call, Band::B14, Mode::Cw, nr);
    let invalid = |k, call: &str, nr: &str| {
        rules(k, ContestMode::Cw).check_multi(&x(call, nr)) == MultiCheck::Invalid
    };
    assert!(invalid(Cqww, "W1AW", "0"));
    assert!(invalid(Cqww, "W1AW", "41"));
    assert!(invalid(Cqww, "W1AW", ""));
    assert!(invalid(Iaru, "W1AW", ""));
    assert!(invalid(ArrlDxDx, "W1AW", "XX"));
    assert!(invalid(ArrlDxW, "ZZ1ZZ", "100"));
    assert!(invalid(Wae, "W1AW", "001"), "outside Europe");
    assert!(invalid(Wpx, "", "001"));
    assert!(invalid(Iota, "W1AW", "001"));
    assert!(invalid(Iota, "W1AW", "001ZZ999"));
    assert!(invalid(JidxDx, "JA1ZLO", "51"));
    assert!(invalid(JidxDx, "JA1ZLO", "0"));
    assert!(invalid(Pedi, "W1AW", "1"));
    // A known one is needed until worked on the band.
    let mut l = log("JA1ZLO", vec![x("W1AW", "5")]);
    let (c, _) = run(Cqww, ContestMode::Cw, &mut l);
    match c.check_multi(&x("K1AA", "5")) {
        MultiCheck::Known {
            needed, worked_on, ..
        } => {
            assert!(!needed);
            assert_eq!(worked_on, vec![Band::B14]);
        }
        MultiCheck::Invalid => panic!("zone 5 is known"),
    }
    assert!(matches!(
        c.check_multi(&q("K1AA", Band::B7, Mode::Cw, "5")),
        MultiCheck::Known { needed: true, .. }
    ));
}

#[test]
fn multi_tables_list_every_multiplier() {
    let mut l = log("JA1ZLO", vec![q("W1AW", Band::B14, Mode::Cw, "5")]);
    let (c, _) = run(Cqww, ContestMode::Cw, &mut l);
    let t = c.multi_table();
    assert_eq!(t[4].0, "5");
    assert_eq!(t[4].2, vec![Band::B14]);
    assert!(t[0].2.is_empty());
    assert!(t.iter().any(|r| r.0 == "K" && r.2 == vec![Band::B14]));
    let iaru = rules(Iaru, ContestMode::Cw).multi_table();
    assert_eq!(iaru.len(), 94);
    assert_eq!(iaru[90].0, "AC");
    assert_eq!(rules(JidxDx, ContestMode::Cw).multi_table().len(), 50);
    assert!(rules(Pedi, ContestMode::All).multi_table().is_empty());
    assert!(rules(Wpx, ContestMode::Cw).multi_table().is_empty());
    let wae = rules(Wae, ContestMode::Cw).multi_table();
    assert!(!wae.is_empty() && wae.iter().all(|r| r.0 != "K"));
    let ten = rules(Arrl10, ContestMode::Mix).multi_table();
    assert!(ten.iter().all(|r| r.0 != "K" && r.0 != "VE" && r.0 != "XE"));
    assert!(ten.iter().any(|r| r.0 == "DL"));
}
