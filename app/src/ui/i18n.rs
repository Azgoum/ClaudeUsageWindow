//! Copy (port of `ui/src/lib/i18n.ts`, EN and FR verbatim).

use super::Lang;

pub struct Strings {
    pub s: &'static str,
    pub w: &'static str,
    pub idle: &'static str,
    pub resets: fn(&str) -> String,
    pub resets_day: fn(&str) -> String,
    pub resets_in: fn(&str) -> String,
    pub limit: fn(&str) -> String,
    pub used: fn(i64) -> String,
    pub expected: fn(i64) -> String,
    pub ahead: fn(i64) -> String,
    pub room: fn(i64) -> String,
    pub on_track: &'static str,
    pub resets_long: fn(&str, &str) -> String,
    pub no_session: &'static str,
    pub starts_next: &'static str,
    pub ago: fn(&str) -> String,
    pub plan_max: &'static str,
    pub plan_ultra: &'static str,
    pub no_limit: &'static str,
    pub no_limit_tip: &'static str,
    pub plan_pro: &'static str,
    pub plan_team: &'static str,
    pub plan_enterprise: &'static str,
    pub err: fn(&str) -> String,
    pub refresh: &'static str,
    pub chip_s: &'static str,
    pub chip_w: &'static str,
    pub surprise: fn(&str) -> String,
    pub chip_both: &'static str,
    pub chip_both_planned: &'static str,
    pub c_title: &'static str,
    pub c_text: &'static str,
    pub c_btn: &'static str,
    pub w_title: &'static str,
    pub w_other: fn(&str) -> String,
    pub again: &'static str,
    pub cancel: &'static str,
    pub e_title: &'static str,
    pub e_text: &'static str,
    pub e_btn: &'static str,
    pub need_firefox_title: &'static str,
    pub need_firefox_text: &'static str,
    pub get_firefox: &'static str,
    pub timed_out: &'static str,
    pub m_refresh: &'static str,
    pub m_org: &'static str,
    pub m_top: &'static str,
    pub m_boot: &'static str,
    pub m_disc: &'static str,
    pub m_quit: &'static str,
}

pub static EN: Strings = Strings {
    s: "5-hour session",
    w: "Weekly",
    idle: "idle",
    resets: |t| format!("resets {t}"),
    resets_day: |t| format!("resets {t}"),
    resets_in: |d| format!("resets in {d}"),
    limit: |d| format!("limit · back in {d}"),
    used: |p| format!("{p}% used"),
    expected: |p| format!("expected {p}%"),
    ahead: |n| format!("{n} pts ahead of pace"),
    room: |n| format!("{n} pts of room"),
    on_track: "on track",
    resets_long: |t, d| format!("Resets {t} (in {d})"),
    no_session: "no session running",
    starts_next: "Starts with your next message",
    ago: |d| format!("updated {d} ago"),
    plan_max: "Max plan",
    plan_ultra: "Ultra plan",
    no_limit: "No 5 h limit",
    no_limit_tip: "No 5-hour limit on the Ultra plan",
    plan_pro: "Pro plan",
    plan_team: "Team plan",
    plan_enterprise: "Enterprise plan",
    err: |d| format!("⚠ claude.ai not responding · retry in {d}"),
    refresh: "Refresh now",
    chip_s: "✓ Session reset · fresh 5 hours",
    chip_w: "✓ New week · weekly limit reset",
    surprise: |d| format!("★ Surprise reset · {d} early"),
    chip_both: "★ Surprise reset · both limits",
    chip_both_planned: "✓ Both limits reset",
    c_title: "See your Claude limits live",
    c_text: "Uses your claude.ai login in your browser. Nothing to copy or paste.",
    c_btn: "Connect to claude.ai",
    w_title: "Waiting for Firefox…",
    w_other: |b| {
        format!("Log in to claude.ai in {b} if you aren't already. We'll pick it up automatically.")
    },
    again: "Open claude.ai again",
    cancel: "Cancel",
    e_title: "Session expired",
    e_text: "Your claude.ai login ran out. Reconnect to keep tracking.",
    e_btn: "Reconnect",
    need_firefox_title: "Firefox needed",
    need_firefox_text: "Claude Limit Checker needs Firefox for now. Chrome support is coming.",
    get_firefox: "Get Firefox",
    timed_out: "Didn't see a login. Try again?",
    m_refresh: "Refresh now",
    m_org: "Organization",
    m_top: "Always on top",
    m_boot: "Start with Windows",
    m_disc: "Disconnect",
    m_quit: "Quit",
};

pub static FR: Strings = Strings {
    s: "Session 5 h",
    w: "Hebdomadaire",
    idle: "inactive",
    resets: |t| format!("reset à {t}"),
    resets_day: |t| format!("reset {t}"),
    resets_in: |d| format!("reset dans {d}"),
    limit: |d| format!("limite · retour dans {d}"),
    used: |p| format!("{p} % utilisé"),
    expected: |p| format!("attendu {p} %"),
    ahead: |n| format!("{n} pts d'avance sur le rythme"),
    room: |n| format!("{n} pts de marge"),
    on_track: "dans le rythme",
    // "à " only before a digit ("14:00"), not before a day name ("ven. 23:59").
    resets_long: |t, d| {
        let at = if t.starts_with(|c: char| c.is_ascii_digit()) {
            "à "
        } else {
            ""
        };
        format!("Reset {at}{t} (dans {d})")
    },
    no_session: "aucune session en cours",
    starts_next: "Démarre avec ton prochain message",
    ago: |d| format!("mis à jour il y a {d}"),
    plan_max: "Forfait Max",
    plan_ultra: "Forfait Ultra",
    no_limit: "Pas de limite 5 h",
    no_limit_tip: "Pas de limite de 5 h sur le forfait Ultra",
    plan_pro: "Forfait Pro",
    plan_team: "Forfait Team",
    plan_enterprise: "Forfait Enterprise",
    err: |d| format!("⚠ claude.ai ne répond pas · nouvel essai dans {d}"),
    refresh: "Actualiser",
    chip_s: "✓ Session remise à zéro · 5 h toutes neuves",
    chip_w: "✓ Nouvelle semaine · limite hebdo remise à zéro",
    surprise: |d| format!("★ Reset surprise · {d} en avance"),
    chip_both: "★ Reset surprise · les deux limites",
    chip_both_planned: "✓ Les deux limites remises à zéro",
    c_title: "Tes limites Claude en direct",
    c_text: "Utilise ta connexion claude.ai dans ton navigateur. Rien à copier-coller.",
    c_btn: "Se connecter à claude.ai",
    w_title: "En attente de Firefox…",
    w_other: |b| {
        format!(
            "Connecte-toi à claude.ai dans {b} si ce n'est pas déjà fait. On le détecte tout seul."
        )
    },
    again: "Rouvrir claude.ai",
    cancel: "Annuler",
    e_title: "Session expirée",
    e_text: "Ta connexion claude.ai a expiré. Reconnecte-toi pour continuer.",
    e_btn: "Se reconnecter",
    need_firefox_title: "Firefox requis",
    need_firefox_text:
        "Claude Limit Checker a besoin de Firefox pour l'instant. Le support de Chrome arrive.",
    get_firefox: "Installer Firefox",
    timed_out: "Aucune connexion détectée. Réessayer ?",
    m_refresh: "Actualiser",
    m_org: "Organisation",
    m_top: "Toujours au premier plan",
    m_boot: "Lancer au démarrage de Windows",
    m_disc: "Se déconnecter",
    m_quit: "Quitter",
};

pub fn t(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        Lang::Fr => &FR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every string of a table, parametrized ones called with "1" / 1, labelled by key.
    fn all(s: &Strings) -> Vec<(&'static str, String)> {
        vec![
            ("s", s.s.into()),
            ("w", s.w.into()),
            ("idle", s.idle.into()),
            ("resets", (s.resets)("1")),
            ("resets_day", (s.resets_day)("1")),
            ("resets_in", (s.resets_in)("1")),
            ("limit", (s.limit)("1")),
            ("used", (s.used)(1)),
            ("expected", (s.expected)(1)),
            ("ahead", (s.ahead)(1)),
            ("room", (s.room)(1)),
            ("on_track", s.on_track.into()),
            ("resets_long", (s.resets_long)("1", "1")),
            ("no_session", s.no_session.into()),
            ("starts_next", s.starts_next.into()),
            ("ago", (s.ago)("1")),
            ("plan_max", s.plan_max.into()),
            ("plan_ultra", s.plan_ultra.into()),
            ("no_limit", s.no_limit.into()),
            ("no_limit_tip", s.no_limit_tip.into()),
            ("plan_pro", s.plan_pro.into()),
            ("plan_team", s.plan_team.into()),
            ("plan_enterprise", s.plan_enterprise.into()),
            ("err", (s.err)("1")),
            ("refresh", s.refresh.into()),
            ("chip_s", s.chip_s.into()),
            ("chip_w", s.chip_w.into()),
            ("surprise", (s.surprise)("1")),
            ("chip_both", s.chip_both.into()),
            ("chip_both_planned", s.chip_both_planned.into()),
            ("c_title", s.c_title.into()),
            ("c_text", s.c_text.into()),
            ("c_btn", s.c_btn.into()),
            ("w_title", s.w_title.into()),
            ("w_other", (s.w_other)("1")),
            ("again", s.again.into()),
            ("cancel", s.cancel.into()),
            ("e_title", s.e_title.into()),
            ("e_text", s.e_text.into()),
            ("e_btn", s.e_btn.into()),
            ("need_firefox_title", s.need_firefox_title.into()),
            ("need_firefox_text", s.need_firefox_text.into()),
            ("get_firefox", s.get_firefox.into()),
            ("timed_out", s.timed_out.into()),
            ("m_refresh", s.m_refresh.into()),
            ("m_org", s.m_org.into()),
            ("m_top", s.m_top.into()),
            ("m_boot", s.m_boot.into()),
            ("m_disc", s.m_disc.into()),
            ("m_quit", s.m_quit.into()),
        ]
    }

    #[test]
    fn strings_non_empty_and_chrome_only_in_need_firefox_text() {
        for lang in [Lang::Fr, Lang::En] {
            for (k, v) in all(t(lang)) {
                assert!(!v.is_empty(), "{lang:?}.{k} is empty");
                if k == "need_firefox_text" {
                    assert!(v.contains("Chrome"));
                    continue;
                }
                assert!(!v.to_lowercase().contains("chrome"), "{lang:?}.{k}");
            }
        }
    }

    #[test]
    fn no_limit_labels() {
        assert_eq!(t(Lang::En).no_limit, "No 5 h limit");
        assert_eq!(t(Lang::Fr).no_limit, "Pas de limite 5 h");
        assert_eq!(t(Lang::En).plan_ultra, "Ultra plan");
        assert_eq!(t(Lang::Fr).plan_ultra, "Forfait Ultra");
    }

    #[test]
    fn format_helpers_keep_prototype_wording() {
        assert_eq!((t(Lang::En).used)(62), "62% used");
        assert_eq!((t(Lang::Fr).used)(62), "62 % utilisé");
        assert_eq!(
            (t(Lang::Fr).resets_long)("14:00", "2h29"),
            "Reset à 14:00 (dans 2h29)"
        );
        assert_eq!(
            (t(Lang::Fr).resets_long)("Ven. 23:59", "3 j"),
            "Reset Ven. 23:59 (dans 3 j)"
        );
    }
}
