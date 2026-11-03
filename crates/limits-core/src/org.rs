use crate::model::Org;

fn is_eligible(o: &Org) -> bool {
    o.capabilities.iter().any(|c| c == "chat")
}

/// Orgs that have the `chat` capability (API-only orgs are excluded).
pub fn eligible_orgs(orgs: &[Org]) -> Vec<&Org> {
    orgs.iter().filter(|o| is_eligible(o)).collect()
}

/// Priority: saved (menu pick) -> lastActiveOrg -> first eligible. Eligible orgs only.
pub fn choose_org<'a>(
    orgs: &'a [Org],
    saved: Option<&str>,
    last_active: Option<&str>,
) -> Option<&'a Org> {
    let eligible = eligible_orgs(orgs);
    let find = |id: Option<&str>| id.and_then(|id| eligible.iter().copied().find(|o| o.uuid == id));
    find(saved).or_else(|| find(last_active)).or_else(|| eligible.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn org(uuid: &str, caps: &[&str]) -> Org {
        Org {
            uuid: uuid.into(),
            name: uuid.into(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn saved_eligible_wins() {
        let orgs = [org("a", &["chat"]), org("b", &["chat"]), org("c", &["chat"])];
        assert_eq!(choose_org(&orgs, Some("c"), Some("b")).unwrap().uuid, "c");
    }

    #[test]
    fn saved_api_only_is_ignored() {
        let orgs = [org("a", &["chat"]), org("b", &["chat"]), org("api", &["api"])];
        assert_eq!(choose_org(&orgs, Some("api"), Some("b")).unwrap().uuid, "b");
    }

    #[test]
    fn unknown_last_active_falls_back_to_first_eligible() {
        let orgs = [org("api", &["api"]), org("a", &["chat"]), org("b", &["chat"])];
        assert_eq!(choose_org(&orgs, None, Some("zzz")).unwrap().uuid, "a");
    }

    #[test]
    fn no_eligible_is_none() {
        let orgs = [org("api", &["api"])];
        assert!(choose_org(&orgs, Some("api"), Some("api")).is_none());
        assert!(choose_org(&[], None, None).is_none());
    }
}
