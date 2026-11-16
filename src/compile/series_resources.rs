//! Native phone journal and localization for the series organiser.
use super::resources::{Handles, document};
use crate::series::RaceSeries;
use anyhow::Result;
use ghostline_quest_builder::nodes as n;
use serde_json::{Value, json};
type Resource = (String, Value);
fn entry(h: &mut Handles, class: &str, id: &str, children: Vec<Value>, fields: Value) -> Value {
    let mut value =
        json!({"$type":class,"id":id,"entries":children,"journalEntryOverrideDataList":[]});
    value
        .as_object_mut()
        .expect("entry")
        .extend(fields.as_object().expect("fields").clone());
    if class == "gameJournalPhoneMessage" {
        value.as_object_mut().expect("message").remove("entries");
    }
    h.wrap(value)
}
pub(super) fn build(series: &RaceSeries, root: &str) -> Result<(Resource, Resource)> {
    let mut h = Handles::default();
    let mut messages = Vec::new();
    let mut strings = Vec::new();
    let mut text = vec![("intro".to_owned(), series.intro_message.clone())];
    for (i, race) in series.races.iter().enumerate() {
        text.push((format!("race_{:02}",i+1),if race.invitation.trim().is_empty(){format!("Race {} of {}: {}. Your next start is marked. Win to advance; completed races remain open for replays.",i+1,series.races.len(),race.title)}else{race.invitation.clone()}));
    }
    text.push(("complete".into(), series.completion_message.clone()));
    for (id, value) in text {
        let key = format!("series_{}_message_{id}", series.id);
        messages.push(entry(&mut h,"gameJournalPhoneMessage",&id,vec![],json!({"attachment":null,"delay":1,"imageId":{"$type":"TweakDBID","$storage":"uint64","$value":"0"},"isQuestImportant":1,"sender":"NPC","text":ghostline_quest_builder::localization::reference(&key)})));
        strings.push((key, value));
    }
    let title_key = format!("series_{}_title", series.id);
    let name_key = format!("series_{}_organiser_name", series.id);
    let thread = entry(
        &mut h,
        "gameJournalPhoneConversation",
        "championship",
        messages,
        json!({"title":ghostline_quest_builder::localization::reference(&title_key)}),
    );
    let contact = entry(
        &mut h,
        "gameJournalContact",
        &format!("{}_organiser", series.id),
        vec![thread],
        json!({"avatarID":n::tweak_id("PhoneAvatars.Avatar_Unknown"),"isCallableDefault":0,"name":ghostline_quest_builder::localization::reference(&name_key),"type":"Texter","useFlatMessageLayout":1}),
    );
    let contacts = entry(
        &mut h,
        "gameJournalPrimaryFolderEntry",
        "contacts",
        vec![contact],
        json!({}),
    );
    let folder = entry(
        &mut h,
        "gameJournalRootFolderEntry",
        "",
        vec![contacts],
        json!({"descriptor":n::resource("base\\journal\\descriptor.journaldesc")}),
    );
    let journal = format!("{root}\\journal\\{}.journal", series.id);
    strings.extend([
        (title_key, series.title.clone()),
        (name_key, series.contact_name.clone()),
    ]);
    let onscreens = format!("{root}\\localization\\en-us\\{}.json", series.id);
    let entries = strings
        .into_iter()
        .map(|(key, text)| ghostline_quest_builder::localization::onscreen_entry(&key, &text))
        .collect::<Vec<_>>();
    Ok((
        (
            journal.clone(),
            document(
                json!({"$type":"gameJournalResource","cookingPlatform":"PLATFORM_PC","entry":folder}),
                &journal,
            ),
        ),
        (
            onscreens.clone(),
            document(
                json!({"$type":"JsonResource","cookingPlatform":"PLATFORM_PC","root":{"HandleId":"0","Data":{"$type":"localizationPersistenceOnScreenEntries","entries":entries}}}),
                &onscreens,
            ),
        ),
    ))
}
