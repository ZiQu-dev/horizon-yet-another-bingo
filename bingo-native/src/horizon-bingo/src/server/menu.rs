//! The lobby menu: the chest `bingo menu` opens for a player, with the original's pages (Root, Features, Goal, Items,
//! Warnings) as slots. Pure logic, no game types: which items each page shows, what a click means, and which page each
//! player's open chest shows. The adapter (`api/src/server.rs`) opens the chest, sends the items this module gives it and
//! runs the command line of a click through the session, so the rules and their checks stay in one place.
//!
//! Layout: every page is four rows (36 slots), so a page change only replaces the items of the open chest. Slot 27 is
//! `Back` and slot 35 is `Close` on the pages that have them. The original's physical menu (`menu/MenuPage.kt`) has no
//! chest equivalent, so the layout is the port's own. The original's game-mode toggles are independent; the port's
//! `Mode` is one exclusive value, so Standard, Inventory and Lockout are three choices.
//!
//! The Items page offers the five difficulty presets of the `difficulty` command, the card's current one selected. The
//! Root page has the reroll and the team settings (auto shuffle, team count), the Features page the starting kit. Each
//! of these runs the command of the same name, so the menu adds no rule of its own.

use super::commands::{parse, requires_op};
use super::difficulty;
use super::multicard::{StalemateBehavior, WinCondition};
use super::options::{EndWhen, Goal, Mode, Options};
use super::teams::PRESETS as TEAM_PRESETS;

/// Rows of every menu page.
pub const ROWS: u8 = 4;
/// Slots of every menu page.
pub const SLOTS: usize = ROWS as usize * 9;
/// The back button of the pages below the root.
pub const BACK_SLOT: usize = 27;
/// The close button, on every page.
pub const CLOSE_SLOT: usize = 35;
/// The root page's reroll button and its start button: the in-world panel runs these two as a click on the slot.
pub const ROOT_REROLL_SLOT: usize = 10;
pub const ROOT_START_SLOT: usize = 22;
/// The root page's shuffle teams slot: shuffles the online players over the rules' team count.
/// The root page's auto shuffle toggle, and the buttons for one team fewer and one more.
pub const ROOT_AUTO_SHUFFLE_SLOT: usize = 12;
pub const ROOT_TEAMS_FEWER_SLOT: usize = 14;
pub const ROOT_TEAMS_MORE_SLOT: usize = 16;
pub const ROOT_SHUFFLE_SLOT: usize = 11;
/// The root page's reset defaults slot: every rule back to its default.
pub const ROOT_DEFAULTS_SLOT: usize = 24;
/// The spawn distances the Features page offers, in chunks.
pub const SPAWN_DISTANCES: [u32; 4] = [10, 25, 50, 100];
/// The reply of a successful `start`, as the session gives it.
/// The reply to a start, the English of `texts::start_success` (`bingo.command.start.success`).
pub const GAME_STARTED: &str = "Starting the game...";
/// The reply to a click on an operator's button by someone else. Same text as `perform_command` gives.
pub const NOT_OPERATOR: &str = "You must be an operator to use this command.";
/// The reply to a click on a setting once the round has started.
pub const ROUND_STARTED: &str = "The round has started, so its rules are read only.";
/// Characters per line of the warnings text.
const WRAP: usize = 40;

/// A page of the menu. `Warnings` carries the refusal text it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Page {
    Root,
    Features,
    Goal,
    Items,
    Warnings(String),
}

/// What a slot does when clicked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Shows the slot, does nothing.
    None,
    /// Runs this command line (the text after `bingo`).
    Run(String),
    /// Shows another page.
    Open(Page),
    /// Closes the menu.
    Close,
}

/// One slot: the item it shows (a namespaced id), its name and lore, and its action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub item: &'static str,
    pub name: String,
    pub lore: Vec<String>,
    pub action: Action,
}

/// What the menu needs to know about the viewer and the round.
#[derive(Debug, Clone, Copy)]
pub struct View<'a> {
    pub rules: &'a Options,
    /// No game has started (`Pregame`): settings can change.
    pub pregame: bool,
    /// The viewer may run operator commands.
    pub operator: bool,
}

/// What a click means, for the adapter to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// Shows this page in the same chest.
    Show(Page),
    Close,
    /// A line for the clicker only.
    Tell(String),
    /// Runs this command line through the session.
    Run(String),
}

/// The chest of a player: which window of the player the menu is in, and the page it shows.
#[derive(Debug, Default)]
pub struct Menus {
    open: Vec<Open>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Open {
    player: u128,
    window: i32,
    page: Page,
}

impl Menus {
    pub const fn new() -> Self {
        Self { open: Vec::new() }
    }

    /// A player's new menu, replacing any they had.
    pub fn open(&mut self, player: u128, window: i32, page: Page) {
        self.open.retain(|open| open.player != player);
        self.open.push(Open {
            player,
            window,
            page,
        });
    }

    /// The page shown in `window` of the player's, if that window is this menu.
    pub fn page(&self, player: u128, window: i32) -> Option<Page> {
        self.open
            .iter()
            .find(|open| open.player == player && open.window == window)
            .map(|open| open.page.clone())
    }

    /// Shows another page in the open window. False if the window is not this menu.
    pub fn turn(&mut self, player: u128, window: i32, page: Page) -> bool {
        match self
            .open
            .iter_mut()
            .find(|open| open.player == player && open.window == window)
        {
            Some(open) => {
                open.page = page;
                true
            }
            None => false,
        }
    }

    /// The menu in `window` closed. False if the window was not this menu.
    pub fn close(&mut self, player: u128, window: i32) -> bool {
        let before = self.open.len();
        self.open
            .retain(|open| !(open.player == player && open.window == window));
        self.open.len() != before
    }

    pub fn clear(&mut self) {
        self.open.clear();
    }
}

/// The slots of a page, each with its index. Only the slots the page shows are listed.
pub fn slots(page: &Page, view: &View) -> Vec<(usize, Slot)> {
    match page {
        Page::Root => root(view),
        Page::Features => features(view),
        Page::Goal => goal(view),
        Page::Items => items(view),
        Page::Warnings(text) => warnings(text, view),
    }
}

/// The items of a page for the chest: `SLOTS` entries, `None` for an empty slot.
pub fn contents(page: &Page, view: &View) -> Vec<Option<Slot>> {
    let mut contents = vec![None; SLOTS];
    for (index, slot) in slots(page, view) {
        contents[index] = Some(slot);
    }
    contents
}

/// What a click on slot `slot` of `page` means. An empty slot does nothing; an operator's command is refused for
/// anyone else, and once the round has started.
pub fn click(page: &Page, slot: usize, view: &View) -> Outcome {
    let Some((_, clicked)) = slots(page, view)
        .into_iter()
        .find(|(index, _)| *index == slot)
    else {
        return Outcome::Nothing;
    };
    match clicked.action {
        Action::None => Outcome::Nothing,
        Action::Open(next) => Outcome::Show(next),
        Action::Close => Outcome::Close,
        Action::Run(line) => gate_command(line, view),
    }
}

/// What running the command `line` means for this viewer: an operator's command is refused for anyone else, and once the
/// round has started. The chest's slots and the in-world panel's tiles both go through this.
pub fn gate_command(line: String, view: &View) -> Outcome {
    let operator_only = parse(&line).is_ok_and(|command| requires_op(&command));
    if !operator_only {
        Outcome::Run(line)
    } else if !view.operator {
        Outcome::Tell(NOT_OPERATOR.to_string())
    } else if !view.pregame {
        Outcome::Tell(ROUND_STARTED.to_string())
    } else {
        Outcome::Run(line)
    }
}

/// Whether a `start` line was refused: the reply is not the start's success line. The menu then shows the refusal on
/// its warnings page, which offers `start ignore_warnings`.
pub fn start_refused(line: &str, reply: &str) -> bool {
    line.split_whitespace().next() == Some("start") && reply != GAME_STARTED
}

/// The item of a slot as an SNBT compound that Horizon's `item_from_snbt` reads: the id, a count of one, the name and
/// the lore as plain text.
pub fn item_snbt(slot: &Slot) -> String {
    let mut components = format!("\"minecraft:custom_name\":{}", snbt_string(&slot.name));
    if !slot.lore.is_empty() {
        let lines: Vec<String> = slot.lore.iter().map(|line| snbt_string(line)).collect();
        components.push_str(&format!(",\"minecraft:lore\":[{}]", lines.join(",")));
    }
    format!(
        "{{id:{},count:1,components:{{{components}}}}}",
        snbt_string(slot.item)
    )
}

/// A double-quoted SNBT string: a backslash and a quote are escaped.
fn snbt_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Word-wraps text at `WRAP` characters, for the lore lines of the warnings.
pub fn wrap(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > WRAP {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The goal as the options line of `/bingo options` words it.
pub fn goal_text(goal: Goal) -> String {
    match goal {
        Goal::Lines(n) => format!("{n} line(s)"),
        Goal::Items(n) => format!("{n} item(s)"),
        Goal::FullCard => "full card".to_string(),
    }
}

/// The card mode as the options line words it.
pub fn mode_text(mode: Mode) -> &'static str {
    match mode {
        Mode::Standard => "standard",
        Mode::Inventory => "inventory",
        Mode::Lockout => "lockout",
    }
}

/// The end condition as the options line words it.
pub fn end_text(end_when: EndWhen) -> String {
    match end_when {
        EndWhen::Never => "never".to_string(),
        EndWhen::FirstWin => "first win".to_string(),
        EndWhen::TeamsWin(n) => format!("{n} teams win"),
        EndWhen::AllWin => "all teams win".to_string(),
    }
}

/// The win condition, in words.
pub fn win_text(win: WinCondition) -> String {
    match win {
        WinCondition::Cards(n) => format!("{n} card(s)"),
        WinCondition::Infinite => "infinite".to_string(),
        WinCondition::ReplaceGoals => "replace goals".to_string(),
    }
}

/// The stalemate behaviour, in words.
pub fn stalemate_text(behaviour: StalemateBehavior) -> &'static str {
    match behaviour {
        StalemateBehavior::EndGame => "end game",
        StalemateBehavior::RerollCard => "reroll card",
        StalemateBehavior::Nothing => "do nothing",
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

/// Lore lines that say who may change a setting, and why not.
fn gate(view: &View) -> Vec<String> {
    let mut notes = Vec::new();
    if !view.operator {
        notes.push("Only operators can change this.".to_string());
    }
    if !view.pregame {
        notes.push("The round has started, so it is read only.".to_string());
    }
    notes
}

/// A setting chosen from several values: the selected one says so in its lore.
fn choice(
    view: &View,
    item: &'static str,
    name: &str,
    description: &str,
    selected: bool,
    command: &str,
) -> (usize, Slot) {
    let mut lore = vec![
        description.to_string(),
        if selected {
            "Selected."
        } else {
            "Click to choose."
        }
        .to_string(),
    ];
    lore.extend(gate(view));
    (0, slot(item, name, lore, Action::Run(command.to_string())))
}

/// A yes/no setting: the slot shows its state and switches it.
fn toggle(
    view: &View,
    item: &'static str,
    label: &str,
    description: &str,
    on: bool,
    word: &str,
) -> (usize, Slot) {
    let next = if on { "off" } else { "on" };
    let mut lore = vec![description.to_string(), format!("Click to switch {next}.")];
    lore.extend(gate(view));
    (
        0,
        slot(
            item,
            &format!("{label}: {}", on_off(on)),
            lore,
            Action::Run(format!("{word} {next}")),
        ),
    )
}

/// A button that shows another page.
fn link(item: &'static str, name: &str, lore: Vec<String>, page: Page) -> (usize, Slot) {
    (0, slot(item, name, lore, Action::Open(page)))
}

/// A slot that does nothing but tell.
fn info(item: &'static str, name: &str, lore: Vec<String>) -> (usize, Slot) {
    (0, slot(item, name, lore, Action::None))
}

fn slot(item: &'static str, name: &str, lore: Vec<String>, action: Action) -> Slot {
    Slot {
        item,
        name: name.to_string(),
        lore,
        action,
    }
}

/// Places a slot at an index.
fn at(index: usize, (_, slot): (usize, Slot)) -> (usize, Slot) {
    (index, slot)
}

fn back_and_close(out: &mut Vec<(usize, Slot)>, back: Page) {
    out.push(at(
        BACK_SLOT,
        link("minecraft:arrow", "Back", Vec::new(), back),
    ));
    out.push(at(CLOSE_SLOT, close_button()));
}

fn close_button() -> (usize, Slot) {
    (
        0,
        slot("minecraft:barrier", "Close", Vec::new(), Action::Close),
    )
}

fn rules_lines(rules: &Options) -> Vec<String> {
    vec![
        format!("Goal: {}", goal_text(rules.goal)),
        format!("Mode: {}", mode_text(rules.mode)),
        format!("End: {}", end_text(rules.end_when)),
        format!("Play to: {}", win_text(rules.win_condition)),
        format!("Stalemate: {}", stalemate_text(rules.stalemate)),
        format!("PvP: {}", on_off(rules.pvp)),
        format!("Keep inventory: {}", on_off(rules.keep_inventory)),
        format!("Elytra: {}", on_off(rules.elytra)),
        format!("Night vision: {}", on_off(rules.night_vision)),
        format!("Spawn distance: {} chunks", rules.spawn_distance),
        format!(
            "Hidden items: {}",
            on_off(rules.hidden_consume.hidden_items)
        ),
        format!(
            "Consume items: {}",
            on_off(rules.hidden_consume.consume_items)
        ),
        format!("Objectives: {}", on_off(rules.objectives)),
    ]
}

fn root(view: &View) -> Vec<(usize, Slot)> {
    let rules = view.rules;
    let mut out = vec![
        at(
            0,
            choice(
                view,
                "minecraft:paper",
                "Standard",
                "An item found stays found.",
                rules.mode == Mode::Standard,
                "mode standard",
            ),
        ),
        at(
            1,
            choice(
                view,
                "minecraft:chest",
                "Inventory",
                "A square counts only while its item is held.",
                rules.mode == Mode::Inventory,
                "mode inventory",
            ),
        ),
        at(
            2,
            choice(
                view,
                "minecraft:iron_bars",
                "Lockout",
                "An item found by one team is lost to the others.",
                rules.mode == Mode::Lockout,
                "mode lockout",
            ),
        ),
        at(
            3,
            toggle(
                view,
                "minecraft:lime_dye",
                "Hidden items",
                "A square shows no item until a team finds it.",
                rules.hidden_consume.hidden_items,
                "mode hidden_items",
            ),
        ),
        at(
            4,
            toggle(
                view,
                "minecraft:redstone",
                "Consume items",
                "A found item is taken from the inventory of the player who found it.",
                rules.hidden_consume.consume_items,
                "mode consume_items",
            ),
        ),
        at(
            6,
            link(
                "minecraft:target",
                "Goal and end",
                vec!["Goal, end condition, win and stalemate.".to_string()],
                Page::Goal,
            ),
        ),
        at(
            7,
            link(
                "minecraft:comparator",
                "Features",
                vec!["PvP, keep inventory, elytra, night vision, spawn distance.".to_string()],
                Page::Features,
            ),
        ),
        at(
            8,
            link(
                "minecraft:map",
                "Items and difficulty",
                vec!["Card difficulty presets.".to_string()],
                Page::Items,
            ),
        ),
        at(
            ROOT_REROLL_SLOT,
            button(
                view,
                "minecraft:ender_pearl",
                "Reroll card",
                "Draws the card again with a new seed.",
                "reroll",
            ),
        ),
        at(
            ROOT_SHUFFLE_SLOT,
            button(
                view,
                "minecraft:ender_eye",
                "Shuffle teams",
                "Shuffles the players over the teams.",
                &format!("shuffleteams {}", rules.team_count),
            ),
        ),
        at(
            ROOT_DEFAULTS_SLOT,
            button(
                view,
                "minecraft:barrier",
                "Reset defaults",
                "Puts every rule back to its default.",
                "defaults",
            ),
        ),
        at(
            ROOT_AUTO_SHUFFLE_SLOT,
            toggle(
                view,
                "minecraft:name_tag",
                "Auto shuffle",
                "The teams are shuffled when the round starts.",
                rules.team_auto_shuffle,
                "autoshuffle",
            ),
        ),
        at(
            13,
            info("minecraft:book", "Current rules", rules_lines(rules)),
        ),
    ];
    out.extend(team_count_slots(view));
    let mut start_lore = vec!["Starts the round, as bingo start does.".to_string()];
    start_lore.extend(gate(view));
    out.push(at(
        ROOT_START_SLOT,
        (
            0,
            slot(
                "minecraft:emerald",
                "Start",
                start_lore,
                Action::Run("start".to_string()),
            ),
        ),
    ));
    out.push(at(CLOSE_SLOT, close_button()));
    out
}

/// The team count of the auto shuffle: a button one fewer (slot 14), the count (15) and a button one more (16). The
/// buttons stop at one team and at the most the teams presets allow, where they only show.
fn team_count_slots(view: &View) -> Vec<(usize, Slot)> {
    let teams = view.rules.team_count;
    let most = TEAM_PRESETS.len();
    let mut out = Vec::new();
    if teams > 1 {
        out.push(at(
            ROOT_TEAMS_FEWER_SLOT,
            button(
                view,
                "minecraft:red_dye",
                "Fewer teams",
                "Makes one team fewer in the auto shuffle.",
                &format!("teams {}", teams - 1),
            ),
        ));
    } else {
        out.push(at(ROOT_TEAMS_FEWER_SLOT, info("minecraft:red_dye", "Fewer teams", vec!["At least one team.".to_string()])));
    }
    out.push(at(
        15,
        info(
            "minecraft:white_banner",
            &format!("Teams: {teams}"),
            vec!["The teams the auto shuffle makes.".to_string()],
        ),
    ));
    if teams < most {
        out.push(at(
            ROOT_TEAMS_MORE_SLOT,
            button(
                view,
                "minecraft:lime_dye",
                "More teams",
                "Makes one team more in the auto shuffle.",
                &format!("teams {}", teams + 1),
            ),
        ));
    } else {
        out.push(at(ROOT_TEAMS_MORE_SLOT, info("minecraft:lime_dye", "More teams", vec![format!("At most {most} teams.")])));
    }
    out
}

fn features(view: &View) -> Vec<(usize, Slot)> {
    let rules = view.rules;
    let mut out = vec![
        at(
            0,
            toggle(
                view,
                "minecraft:diamond_sword",
                "PvP",
                "Players can hurt each other.",
                rules.pvp,
                "pvp",
            ),
        ),
        at(
            1,
            toggle(
                view,
                "minecraft:ender_chest",
                "Keep inventory",
                "Players keep their items when they die.",
                rules.keep_inventory,
                "keepinventory",
            ),
        ),
        at(
            2,
            toggle(
                view,
                "minecraft:elytra",
                "Elytra",
                "Players can fly with elytra.",
                rules.elytra,
                "elytra",
            ),
        ),
        at(
            3,
            toggle(
                view,
                "minecraft:glowstone_dust",
                "Night vision",
                "Players see in the dark.",
                rules.night_vision,
                "nightvision",
            ),
        ),
        at(4, kit_slot(view)),
    ];
    for (offset, chunks) in SPAWN_DISTANCES.into_iter().enumerate() {
        let name = format!("Spawn distance: {chunks} chunks");
        let selected = rules.spawn_distance == chunks;
        out.push(at(
            5 + offset,
            choice(
                view,
                "minecraft:compass",
                &name,
                "Distance between team spawns.",
                selected,
                &format!("spawndistance {chunks}"),
            ),
        ));
    }
    back_and_close(&mut out, Page::Root);
    out
}

/// The starting kit. The kit has no on or off of its own: it is on when it has items. Clicking clears it (`kit clear`,
/// before the start); turning it on takes `bingo kit add`, which the lore names.
fn kit_slot(view: &View) -> (usize, Slot) {
    let kit = &view.rules.kit;
    if kit.is_empty() {
        let mut lore = vec!["No items. Add some with bingo kit add <item> [count].".to_string()];
        lore.extend(gate(view));
        return info("minecraft:chest", "Starting kit: off", lore);
    }
    let mut lore = vec![format!(
        "{} kind(s) of item are given to team players at the start.",
        kit.items.len()
    )];
    lore.push("Click to clear the kit.".to_string());
    lore.extend(gate(view));
    (
        0,
        slot("minecraft:chest", "Starting kit: on", lore, Action::Run("kit clear".to_string())),
    )
}

/// A button that runs `command`, with the gate lore.
fn button(view: &View, item: &'static str, name: &str, description: &str, command: &str) -> (usize, Slot) {
    let mut lore = vec![description.to_string()];
    lore.extend(gate(view));
    (0, slot(item, name, lore, Action::Run(command.to_string())))
}

fn goal(view: &View) -> Vec<(usize, Slot)> {
    let rules = view.rules;
    let goals = [
        (Goal::Lines(1), "1 line", "Complete one line to win."),
        (Goal::Lines(2), "2 lines", "Complete two lines to win."),
        (Goal::Lines(3), "3 lines", "Complete three lines to win."),
        (Goal::Items(5), "5 items", "Find five squares."),
        (Goal::Items(10), "10 items", "Find ten squares."),
        (Goal::Items(15), "15 items", "Find fifteen squares."),
        (Goal::FullCard, "Full card", "Find every square."),
    ];
    let mut out = Vec::new();
    for (index, (goal, name, description)) in goals.into_iter().enumerate() {
        let command = match goal {
            Goal::Lines(n) => format!("goal lines {n}"),
            Goal::Items(n) => format!("goal items {n}"),
            Goal::FullCard => "goal full_card".to_string(),
        };
        let item = if matches!(goal, Goal::Lines(_)) {
            "minecraft:string"
        } else if goal == Goal::FullCard {
            "minecraft:nether_star"
        } else {
            "minecraft:diamond"
        };
        out.push(at(
            index,
            choice(view, item, name, description, rules.goal == goal, &command),
        ));
    }
    for (index, minutes) in [(9, "off"), (10, "15"), (11, "30"), (12, "60")] {
        let name = if minutes == "off" {
            "No time limit".to_string()
        } else {
            format!("Time limit: {minutes} minutes")
        };
        out.push(at(
            index,
            choice(
                view,
                "minecraft:clock",
                &name,
                "Ends the round when the time runs out.",
                false,
                &format!("timelimit {minutes}"),
            ),
        ));
    }
    let ends = [
        (
            EndWhen::Never,
            "Never",
            "The round ends only with end or the time limit.",
        ),
        (
            EndWhen::FirstWin,
            "First win",
            "The round ends when a team wins.",
        ),
        (
            EndWhen::AllWin,
            "All win",
            "The round ends when every team has won.",
        ),
        (
            EndWhen::TeamsWin(2),
            "Two teams win",
            "The round ends when two teams have won.",
        ),
    ];
    for (index, (end, name, description)) in ends.into_iter().enumerate() {
        let command = match end {
            EndWhen::Never => "endwhen never".to_string(),
            EndWhen::FirstWin => "endwhen first_win".to_string(),
            EndWhen::AllWin => "endwhen all_win".to_string(),
            EndWhen::TeamsWin(n) => format!("endwhen teams_win {n}"),
        };
        out.push(at(
            13 + index,
            choice(
                view,
                "minecraft:ender_eye",
                name,
                description,
                rules.end_when == end,
                &command,
            ),
        ));
    }
    let wins = [
        (
            WinCondition::Cards(1),
            "One card",
            "A team wins by completing one card.",
            "playto cards 1",
        ),
        (
            WinCondition::Cards(2),
            "Two cards",
            "A team wins by completing two cards.",
            "playto cards 2",
        ),
        (
            WinCondition::Cards(3),
            "Three cards",
            "A team wins by completing three cards.",
            "playto cards 3",
        ),
        (
            WinCondition::Infinite,
            "Infinite",
            "Cards are never completed to win.",
            "playto infinite",
        ),
        (
            WinCondition::ReplaceGoals,
            "Replace goals",
            "Scored goals are replaced.",
            "playto replace_goals",
        ),
    ];
    for (index, (win, name, description, command)) in wins.into_iter().enumerate() {
        out.push(at(
            18 + index,
            choice(
                view,
                "minecraft:book",
                name,
                description,
                rules.win_condition == win,
                command,
            ),
        ));
    }
    let stalemates = [
        (
            StalemateBehavior::EndGame,
            "End game",
            "A stalemate ends the game.",
            "stalemate end_game",
        ),
        (
            StalemateBehavior::RerollCard,
            "Reroll card",
            "A stalemate gives a new card.",
            "stalemate reroll_card",
        ),
        (
            StalemateBehavior::Nothing,
            "Do nothing",
            "A stalemate changes nothing.",
            "stalemate do_nothing",
        ),
    ];
    for (index, (behaviour, name, description, command)) in stalemates.into_iter().enumerate() {
        out.push(at(
            23 + index,
            choice(
                view,
                "minecraft:redstone",
                name,
                description,
                rules.stalemate == behaviour,
                command,
            ),
        ));
    }
    back_and_close(&mut out, Page::Root);
    out
}

/// The difficulty presets of the Items page: one `choice` per preset, running `difficulty <name>`. The card's current
/// preset is the selected one.
fn difficulty_slots(view: &View) -> Vec<(usize, Slot)> {
    const SLOTS: [usize; 5] = [11, 12, 14, 15, 16];
    const ITEMS: [&str; 5] = [
        "minecraft:lime_dye",
        "minecraft:yellow_dye",
        "minecraft:orange_dye",
        "minecraft:red_dye",
        "minecraft:black_dye",
    ];
    difficulty::PRESETS
        .iter()
        .enumerate()
        .map(|(i, (name, counts))| {
            let tiers = difficulty::TIER_NAMES
                .iter()
                .zip(counts.iter())
                .map(|(tier, n)| format!("{tier} {n}"))
                .collect::<Vec<_>>()
                .join(", ");
            let mut chars = name.chars();
            let title = match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            };
            at(
                SLOTS[i],
                choice(
                    view,
                    ITEMS[i],
                    &title,
                    &format!("Squares per tier: {tiers}."),
                    view.rules.tier_counts == *counts,
                    &format!("difficulty {name}"),
                ),
            )
        })
        .collect()
}

fn items(view: &View) -> Vec<(usize, Slot)> {
    let mut out = vec![at(
        13,
        info(
            "minecraft:map",
            "Card difficulty",
            vec!["Pick a preset, or set the tiers with difficulty <S> <A> <B> <C> <D>.".to_string()],
        ),
    )];
    out.extend(difficulty_slots(view));
    back_and_close(&mut out, Page::Root);
    out
}

fn warnings(text: &str, view: &View) -> Vec<(usize, Slot)> {
    let mut out = vec![at(13, info("minecraft:bell", "Start refused", wrap(text)))];
    let mut lore = vec!["Starts the round. Players not on a team become spectators.".to_string()];
    lore.extend(gate(view));
    out.push(at(
        21,
        (
            0,
            slot(
                "minecraft:emerald",
                "Start anyway",
                lore,
                Action::Run("start ignore_warnings".to_string()),
            ),
        ),
    ));
    back_and_close(&mut out, Page::Root);
    out
}
