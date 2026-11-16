//! Native championship progression; permanent facts never enter race reset loops.
use super::*;
use crate::series::RaceSeries;

fn message(b: &mut Builder, source: Signal, path: &str, class: &str) -> Result<Signal> {
    let target = n::journal(&mut b.g, b.next, path, class, 1)?;
    b.next += 1;
    let signal = Signal {
        node: target,
        out: 0,
    };
    b.ports.insert(target, vec!["Out".into()]);
    port(b, source, signal, "Active")
}
fn send_once(b: &mut Builder, source: Signal, series: &RaceSeries, name: &str) -> Result<Signal> {
    let fact = format!("series_{}_message_{name}", series.id);
    let check = b.fact_condition(&fact, "Equal", 0, true)?;
    b.link(source, check, "In")?;
    let sent = message(
        b,
        check,
        &format!("contacts/{}_organiser/championship/{name}", series.id),
        "gameJournalPhoneMessage",
    )?;
    let done = b.fact(&fact, 1)?;
    b.link(sent, done, "In")?;
    let join = b.fact(&fact, 1)?;
    b.link(done, join, "In")?;
    b.link(Builder::out(check, 1), join, "In")?;
    Ok(join)
}
pub(super) fn compile(series: &RaceSeries, races: &[Value], depot: &str) -> Result<Value> {
    let mut b = Builder::new(&[])?;
    let entered = b.fact(&format!("series_{}_entered", series.id), 1)?;
    b.link(b.start, entered, "In")?;
    let ready = b.typed_condition(
        "questContentCondition",
        json!({"$type":"questContentLock_ConditionType","isContentBlocked":0}),
        false,
    )?;
    b.link(entered, ready, "In")?;
    let available = b.fact(&format!("series_{}_ready", series.id), 1)?;
    b.link(ready, available, "In")?;
    // Keep each root alive as a child. A terminating series output would kill replays.
    for race in races {
        let child = b.child(binding(race, "root_phase")?, &[], binding(race, "prefab")?)?;
        b.link(available, child, "In1")?;
    }
    let contact = message(
        &mut b,
        available,
        &format!("contacts/{}_organiser", series.id),
        "gameJournalContact",
    )?;
    let thread = message(
        &mut b,
        contact,
        &format!("contacts/{}_organiser/championship", series.id),
        "gameJournalPhoneConversation",
    )?;
    let mut source = send_once(&mut b, thread, series, "intro")?;
    for (index, race) in races.iter().enumerate() {
        let membership = series.membership(index, binding(race, "id")?);
        step!(b, source, fact(&membership.unlock_fact(), 1));
        step!(
            b,
            source,
            fact_condition(
                &format!("{}_waiting_armed", binding(race, "id")?),
                "Greater",
                0,
                false
            )
        );
        source = send_once(&mut b, source, series, &format!("race_{:02}", index + 1))?;
        step!(
            b,
            source,
            fact_condition(&membership.win_fact(), "Greater", 0, false)
        );
        step!(
            b,
            source,
            fact_condition(
                &format!("{}_settled", binding(race, "id")?),
                "Greater",
                0,
                false
            )
        );
    }
    let paid_fact = format!("series_{}_reward_paid", series.id);
    let paid = b.fact_condition(&paid_fact, "Equal", 0, true)?;
    b.link(source, paid, "In")?;
    let mut pay = paid;
    // Lock saves across the inventory mutation and durable receipt.
    step!(b, pay, lock(&format!("series_{}_reward", series.id), true));
    let local =
        b.g.wrap(json!({"$type":"questUniversalRef","refLocalPlayer":true}));
    let params=b.g.wrap(json!({"$type":"questAddRemoveItem_NodeTypeParams","entityRef":local,"isPlayer":0,"itemID":n::tweak_id("Items.money"),"quantity":series.reward_eddies,"nodeType":"AddItem","objectRef":entity("",&[]),"flagItemAddedCallbackAsSilent":0,"itemIDsToIgnoreOnRemove":[],"removeAllQuantity":0,"sendNotification":1,"tagsToIgnoreOnRemove":[],"tagToRemove":n::cname("None")}));
    step!(
        b,
        pay,
        manager(
            "questItemManagerNodeDefinition",
            json!({"$type":"questAddRemoveItem_NodeType","params":[params]})
        )
    );
    step!(b, pay, fact(&paid_fact, 1));
    step!(b, pay, fact(&format!("series_{}_completed", series.id), 1));
    step!(b, pay, lock(&format!("series_{}_reward", series.id), false));
    let done = b.fact(&format!("series_{}_completed", series.id), 1)?;
    b.link(pay, done, "In")?;
    b.link(Builder::out(paid, 1), done, "In")?;
    send_once(&mut b, done, series, "complete")?;
    b.g.document(depot, &[])
}
