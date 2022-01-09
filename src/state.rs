use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::mem;
use std::str::FromStr;

use chrono::{Local, NaiveDate};
use wasm_bindgen::{JsValue};
use web_sys::{window, Window};

const WORDS: &str = include_str!("../word-list.txt");
const DAILY_WORDS: &str = include_str!("../daily-words.txt");
const EMPTY: char = '\u{00a0}'; // &nbsp;
const SUCCESS_EMOJIS: [&str; 8] = ["🥳", "🤩", "🤗", "🎉", "😊", "😺", "😎", "👏"];
pub const DEFAULT_WORD_LENGTH: usize = 5;
pub const DEFAULT_MAX_GUESSES: usize = 6;

fn parse_words(words: &str, word_length: usize) -> Vec<Vec<char>> {
    words
        .lines()
        .filter(|word| word.chars().count() == word_length)
        .map(|word| word.chars().collect())
        .collect()
}

#[derive(PartialEq, Clone, Copy)]
pub enum GameMode {
    Classic,
    Relay,
    DailyWord,
}

impl FromStr for GameMode {
    type Err = ();

    fn from_str(input: &str) -> Result<GameMode, Self::Err> {
        match input {
            "classic" => Ok(GameMode::Classic),
            "relay" => Ok(GameMode::Relay),
            "daily_word" => Ok(GameMode::DailyWord),
            _ => Err(()),
        }
    }
}

impl fmt::Display for GameMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GameMode::Classic => write!(f, "classic"),
            GameMode::Relay => write!(f, "relay"),
            GameMode::DailyWord => write!(f, "daily_word"),
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum CharacterState {
    Correct,
    Absent,
    Unknown,
}

#[derive(Clone, PartialEq)]
pub enum TileState {
    Correct,
    Absent,
    Present,
    Unknown,
}

impl fmt::Display for TileState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TileState::Correct => write!(f, "correct"),
            TileState::Absent => write!(f, "absent"),
            TileState::Present => write!(f, "present"),
            TileState::Unknown => write!(f, "unknown"),
        }
    }
}

#[derive(Clone)]
pub struct DailyWordHistory {
    date: NaiveDate,
    word: String,
    guesses: Vec<Vec<char>>,
    current_guess: usize,
    is_guessing: bool,
    is_winner: bool,
}

pub struct State {
    pub word_list: Vec<Vec<char>>,
    pub word: Vec<char>,

    pub word_length: usize,
    pub max_guesses: usize,

    pub is_guessing: bool,
    pub is_winner: bool,
    pub is_unknown: bool,
    pub is_reset: bool,
    pub is_help_visible: bool,
    pub is_menu_visible: bool,

    pub daily_word_history: HashMap<NaiveDate, DailyWordHistory>,

    pub game_mode: GameMode,
    pub previous_game_mode: GameMode,

    pub message: String,

    pub known_states: Vec<HashMap<(char, usize), CharacterState>>,
    pub known_at_least_counts: Vec<HashMap<char, usize>>,
    pub discovered_characters: HashSet<char>,

    pub guesses: Vec<Vec<(char, TileState)>>,
    pub previous_guesses: Vec<Vec<(char, TileState)>>,
    pub current_guess: usize,

    pub streak: usize,
    pub max_streak: usize,
    pub total_played: usize,
    pub total_solved: usize,
}

impl State {
    pub fn new(word_length: usize, max_guesses: usize) -> Self {
        let word_list = parse_words(WORDS, word_length);
        let word = word_list.choose(&mut rand::thread_rng()).unwrap().clone();
        let guesses = std::iter::repeat(Vec::with_capacity(word_length))
            .take(max_guesses)
            .collect::<Vec<_>>();

        let known_states = std::iter::repeat(HashMap::new())
            .take(max_guesses)
            .collect::<Vec<_>>();

        let known_at_least_counts = std::iter::repeat(HashMap::new())
            .take(max_guesses)
            .collect::<Vec<_>>();

        Self {
            word,
            word_list,

            word_length,
            max_guesses,

            is_guessing: true,
            is_winner: false,
            is_unknown: false,
            is_reset: false,
            is_menu_visible: false,
            is_help_visible: false,

            daily_word_history: HashMap::new(),

            game_mode: GameMode::Classic,
            previous_game_mode: GameMode::Classic,

            message: EMPTY.to_string(),

            known_states,
            known_at_least_counts,
            discovered_characters: HashSet::new(),

            guesses,
            previous_guesses: Vec::new(),
            current_guess: 0,
            streak: 0,
            max_streak: 0,
            total_played: 0,
            total_solved: 0,
        }
    }

    pub fn update_current_guess_tiles(&mut self) {
        for (index, (character, tile_state)) in self.guesses[self.current_guess].iter_mut().enumerate() {
            match self.known_states[self.current_guess].get(&(*character, index)) {
                Some(CharacterState::Correct) => {
                    *tile_state = TileState::Correct;
                }
                Some(CharacterState::Absent) => {
                    *tile_state = TileState::Absent;
                }
                _ => {
                    let is_count_unknown = !self.known_at_least_counts[self.current_guess]
                        .contains_key(character);

                    let is_absent = is_count_unknown
                        && self.known_states[self.current_guess]
                            .iter()
                            .any(|((c, _index), state)| {
                                c == character && state == &CharacterState::Absent
                            });

                    if is_absent {
                        *tile_state = TileState::Absent;
                    } else if self.discovered_characters.contains(character) {
                        *tile_state = TileState::Present;
                    } else {
                        *tile_state = TileState::Unknown;
                    }
                }
            }
        }
    }

    pub fn update_row_tiles(&mut self, row: usize) {
        if let Some(guess) = self.guesses.get_mut(row) {
            let mut revealed_count_on_row: HashMap<char, usize> = HashMap::with_capacity(self.word_length);

            for (index, (character, _)) in guess.iter().enumerate() {
                if let Some(CharacterState::Correct) =
                    self.known_states[row].get(&(*character, index))
                {
                    revealed_count_on_row
                        .entry(*character)
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
            }

            for (index, (character, tile_state)) in guess.into_iter().enumerate() {
                match self.known_states[row].get(&(*character, index)) {
                    Some(CharacterState::Correct) => {
                        *tile_state = TileState::Correct;
                    }
                    Some(CharacterState::Absent) => {
                        let revealed = revealed_count_on_row
                            .entry(*character)
                            .and_modify(|count| *count += 1)
                            .or_insert(1);

                        let at_least = self.known_at_least_counts[row]
                            .get(character)
                            .unwrap_or(&0);

                        if *revealed <= *at_least {
                            *tile_state = TileState::Present;
                        } else {
                            *tile_state = TileState::Absent;
                        }
                    }
                    _ => {
                        *tile_state = TileState::Unknown;
                    }
                }
            }
        }
    }

    pub fn map_keyboard_tilestate(&self, key: &char) -> TileState {
        let is_correct = self.known_states[self.current_guess]
            .iter()
            .any(|((c, _index), state)| c == key && state == &CharacterState::Correct);
        if is_correct {
            return TileState::Correct;
        }

        let is_count_unknown =
            !self.known_at_least_counts[self.current_guess].contains_key(key);

        let is_absent = is_count_unknown
            && self.known_states[self.current_guess]
                .iter()
                .any(|((c, _index), state)| c == key && state == &CharacterState::Absent);

        if is_absent {
            return TileState::Absent;
        } else if self.discovered_characters.contains(key) {
            return TileState::Present
        } else {
            return TileState::Unknown;
        }
    }

    pub fn submit_current_guess(&mut self) {
        for (index, (character, _)) in self.guesses[self.current_guess].iter().enumerate() {
            let known = self.known_states[self.current_guess]
                .entry((*character, index))
                .or_insert(CharacterState::Unknown);

            if self.word[index] == *character {
                *known = CharacterState::Correct;
            } else {
                *known = CharacterState::Absent;

                if self.word.contains(character) {
                    let at_least = self.known_at_least_counts[self.current_guess]
                        .entry(*character)
                        .or_insert(0);
                    // At least the same amount of characters as in the word are highlighted
                    let count_in_word = self.word.iter().filter(|c| *c == character).count();
                    let count_in_guess = self.guesses[self.current_guess]
                        .iter()
                        .filter(|(c, _)| c == character)
                        .count();
                    if count_in_guess >= count_in_word {
                        if count_in_word > *at_least {
                            *at_least = count_in_word;
                        }
                    } else if count_in_guess > *at_least {
                        *at_least = count_in_guess;
                    }

                    self.discovered_characters.insert(*character);
                }
            }
        }

        // Copy the previous knowledge to the next round
        if self.current_guess < self.max_guesses - 1 {
            let next = self.current_guess + 1;
            self.known_states[next] = self.known_states[self.current_guess].clone();
            self.known_at_least_counts[next] =
                self.known_at_least_counts[self.current_guess].clone();
        }

        self.update_row_tiles(self.current_guess);
    }

    pub fn persist_settings(&mut self) -> Result<(), JsValue> {
        let window: Window = window().expect("window not available");
        let local_storage = window.local_storage().expect("local storage not available");
        if let Some(local_storage) = local_storage {
            local_storage.set_item("game_mode", &self.game_mode.to_string())?;
            local_storage.set_item("word_length", format!("{}", self.word_length).as_str())?;
        }

        Ok(())
    }

    pub fn persist_stats(&mut self) -> Result<(), JsValue> {
        let window: Window = window().expect("window not available");
        let local_storage = window.local_storage().expect("local storage not available");
        if let Some(local_storage) = local_storage {
            local_storage.set_item("streak", &format!("{}", self.streak))?;
            local_storage.set_item("max_streak", &format!("{}", self.max_streak))?;
            local_storage.set_item("total_played", &format!("{}", self.total_played))?;
            local_storage.set_item("total_solved", &format!("{}", self.total_solved))?;
        }

        Ok(())
    }

    pub fn persist_game(&mut self) -> Result<(), JsValue> {
        let window: Window = window().expect("window not available");
        let local_storage = window.local_storage().expect("local storage not available");
        if let Some(local_storage) = local_storage {
            local_storage.set_item("word", &self.word.iter().collect::<String>())?;
            local_storage.set_item("word_length", &format!("{}", self.word_length))?;
            local_storage.set_item("current_guess", &format!("{}", self.current_guess))?;
            local_storage.set_item(
                "guesses",
                &self
                    .guesses
                    .iter()
                    .map(|guess| guess.iter().map(|(c, _)| c).collect::<String>())
                    .collect::<Vec<String>>()
                    .join(","),
            )?;
            local_storage.set_item("message", &self.message)?;
            local_storage.set_item("is_guessing", format!("{}", self.is_guessing).as_str())?;
            local_storage.set_item("is_winner", format!("{}", self.is_winner).as_str())?;
        }

        Ok(())
    }

    pub fn persist_single_daily_word(&mut self, date: &NaiveDate) -> Result<(), JsValue> {
        let window: Window = window().expect("window not available");
        let local_storage = window.local_storage().expect("local storage not available");

        if let Some(local_storage) = local_storage {
            if let Some(history) = self.daily_word_history.get(date) {
                local_storage.set_item(
                    &format!("daily_word_history[{}]", date.format("%Y-%m-%d")),
                    &format!(
                        "{}|{}|{}|{}|{}|{}",
                        history.word,
                        history.date.format("%Y-%m-%d"),
                        history
                            .guesses
                            .iter()
                            .map(|guess| guess.iter().collect::<String>())
                            .collect::<Vec<_>>()
                            .join(","),
                        history.current_guess,
                        history.is_guessing,
                        history.is_winner
                    ),
                )?;
            }

            local_storage.set_item(
                "daily_word_history",
                &format!(
                    "{}",
                    &self
                        .daily_word_history
                        .keys()
                        .map(|date| date.format("%Y-%m-%d").to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            )?;

            local_storage.set_item("total_played", &format!("{}", self.total_played))?;
            local_storage.set_item("total_solved", &format!("{}", self.total_solved))?;
        }

        Ok(())
    }

    pub fn rehydrate_daily_word(&mut self) {
        self.word = self.get_daily_word();
        if self.word.len() != self.word_length {
            self.word_length = self.word.len();
            self.word_list = parse_words(WORDS, self.word_length);
        }

        let today = Local::now().naive_local().date();
        if let Some(solve) = self.daily_word_history.get(&today).cloned() {
            for (guess_index, guess) in solve.guesses.iter().enumerate() {
                self.guesses[guess_index] = guess.iter().map(|c| (*c, TileState::Unknown)).collect();
                self.current_guess = guess_index;
                self.submit_current_guess();
            }
            self.is_guessing = solve.is_guessing;
            self.is_winner = solve.is_winner;
            self.current_guess = solve.current_guess;

            if !self.is_guessing {
                self.message = "Uusi sanuli huomenna!".to_owned();
            } else {
                self.message = EMPTY.to_string()
            }
        }
    }

    pub fn rehydrate_game(&mut self) -> Result<(), JsValue> {
        let window: Window = window().expect("window not available");
        if let Some(local_storage) = window.local_storage().expect("local storage not available") {
            let word_length_item = local_storage.get_item("word_length")?;
            if let Some(word_length_str) = word_length_item {
                if let Ok(word_length) = word_length_str.parse::<usize>() {
                    if word_length != self.word_length {
                        self.word_list = parse_words(WORDS, word_length);
                    }
                    self.word_length = word_length;
                }
            }

            let word = local_storage.get_item("word")?;
            if let Some(word) = word {
                self.word = word.chars().collect();
            } else {
                local_storage.set_item("word", &self.word.iter().collect::<String>())?;
            }
            let is_guessing_item = local_storage.get_item("is_guessing")?;
            if let Some(is_guessing_str) = is_guessing_item {
                if let Ok(is_guessing) = is_guessing_str.parse::<bool>() {
                    self.is_guessing = is_guessing;
                }
            }

            let is_winner_item = local_storage.get_item("is_winner")?;
            if let Some(is_winner_str) = is_winner_item {
                if let Ok(is_winner) = is_winner_str.parse::<bool>() {
                    self.is_winner = is_winner;
                }
            }

            let guesses_item = local_storage.get_item("guesses")?;
            if let Some(guesses_str) = guesses_item {
                let previous_guesses = guesses_str.split(',').map(|guess| guess.chars().map(|c| (c, TileState::Unknown)).collect());

                for (guess_index, guess) in previous_guesses.enumerate() {
                    self.guesses[guess_index] = guess;
                    self.current_guess = guess_index;
                    self.submit_current_guess();
                }
            }

            let current_guess_item = local_storage.get_item("current_guess")?;
            if let Some(current_guess_str) = current_guess_item {
                if let Ok(current_guess) = current_guess_str.parse::<usize>() {
                    self.current_guess = current_guess;
                }
            }
        }

        Ok(())
    }

    pub fn rehydrate(&mut self) -> Result<(), JsValue> {
        let window: Window = window().expect("window not available");
        if let Some(local_storage) = window.local_storage().expect("local storage not available") {
            // Common state
            let game_mode_item = local_storage.get_item("game_mode")?;
            if let Some(game_mode_str) = game_mode_item {
                if let Ok(new_mode) = game_mode_str.parse::<GameMode>() {
                    self.previous_game_mode = mem::replace(&mut self.game_mode, new_mode);
                }
            }

            let daily_word_history_item = local_storage.get_item("daily_word_history")?;
            if let Some(daily_word_history_str) = daily_word_history_item {
                if daily_word_history_str.len() != 0 {
                    daily_word_history_str.split(',').for_each(|date_str| {
                        let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").unwrap();
                        let daily_item = local_storage
                            .get_item(&format!("daily_word_history[{}]", date_str))
                            .unwrap();
                        if let Some(daily_str) = daily_item {
                            let parts = daily_str.split('|').collect::<Vec<&str>>();

                            // AIVAN|2022-01-07|KOIRA,AVAIN,AIVAN,,,|2|true|true
                            let word = parts[0];
                            let guesses = parts[2]
                                .split(',')
                                .map(|guess| guess.chars().collect::<Vec<_>>())
                                .collect::<Vec<_>>();
                            let current_guess = parts[3].parse::<usize>().unwrap();
                            let is_guessing = parts[4].parse::<bool>().unwrap();
                            let is_winner = parts[5].parse::<bool>().unwrap();

                            let history = DailyWordHistory {
                                word: word.to_string(),
                                date,
                                guesses: guesses,
                                current_guess: current_guess,
                                is_guessing: is_guessing,
                                is_winner: is_winner,
                            };

                            self.daily_word_history.insert(date, history);
                        }
                    });
                }
            }

            let message_item = local_storage.get_item("message")?;
            if let Some(message_str) = message_item {
                self.message = message_str;
            }

            // Stats
            let streak_item = local_storage.get_item("streak")?;
            if let Some(streak_str) = streak_item {
                if let Ok(streak) = streak_str.parse::<usize>() {
                    self.streak = streak;
                }
            }

            let max_streak_item = local_storage.get_item("max_streak")?;
            if let Some(max_streak_str) = max_streak_item {
                if let Ok(max_streak) = max_streak_str.parse::<usize>() {
                    self.max_streak = max_streak;
                }
            }

            let total_played_item = local_storage.get_item("total_played")?;
            if let Some(total_played_str) = total_played_item {
                if let Ok(total_played) = total_played_str.parse::<usize>() {
                    self.total_played = total_played;
                }
            }

            let total_solved_item = local_storage.get_item("total_solved")?;
            if let Some(total_solved_str) = total_solved_item {
                if let Ok(total_solved) = total_solved_str.parse::<usize>() {
                    self.total_solved = total_solved;
                }
            }

            // Gamemode specific
            match self.game_mode {
                GameMode::DailyWord => {
                    self.rehydrate_daily_word();
                }
                GameMode::Classic | GameMode::Relay => {
                    self.rehydrate_game()?;
                }
            }
        }

        Ok(())
    }

    pub fn get_random_word(&self) -> Vec<char> {
        self.word_list
            .choose(&mut rand::thread_rng())
            .unwrap()
            .clone()
    }

    pub fn get_daily_word_index(&self) -> usize {
        let epoch = NaiveDate::from_ymd(2022, 1, 07); // Epoch of the daily word mode, index 0
        Local::now()
            .naive_local()
            .date()
            .signed_duration_since(epoch)
            .num_days() as usize
    }

    pub fn get_daily_word(&self) -> Vec<char> {
        DAILY_WORDS
            .lines()
            .nth(self.get_daily_word_index())
            .unwrap()
            .chars()
            .collect()
    }

    pub fn push_character(&mut self, character: char) -> bool {
        if !self.is_guessing || self.guesses[self.current_guess].len() >= self.word_length {
            return false;
        }

        self.clear_message();
        self.guesses[self.current_guess].push((character, TileState::Unknown));

        self.update_current_guess_tiles();
        true
    }

    pub fn pop_character(&mut self) -> bool {
        if !self.is_guessing || self.guesses[self.current_guess].is_empty() {
            return false;
        }

        self.clear_message();
        self.guesses[self.current_guess].pop();

        self.update_current_guess_tiles();
        true
    }

    fn is_guess_allowed(&self) -> bool {
        self.is_guessing && self.guesses[self.current_guess].len() == self.word_length
    }

    fn is_guess_real_word(&self) -> bool {
        self.word_list.contains(&self.guesses[self.current_guess].iter().map(|(c, _)| *c).collect())
    }

    fn is_correct_word(&self) -> bool {
        self.guesses[self.current_guess].iter().map(|(c, _)| *c).collect::<Vec<char>>() == self.word
    }

    fn is_game_ended(&self) -> bool {
        self.is_winner || self.current_guess == self.max_guesses - 1
    }

    fn clear_message(&mut self) {
        self.is_unknown = false;
        self.message = EMPTY.to_string();
    }

    fn set_game_end_message(&mut self) {
        if self.is_winner {
            if self.game_mode == GameMode::DailyWord {
                self.message = format!(
                    "Löysit päivän sanulin! {}",
                    SUCCESS_EMOJIS.choose(&mut rand::thread_rng()).unwrap()
                );
            } else {
                self.message = format!(
                    "Löysit sanan! {}",
                    SUCCESS_EMOJIS.choose(&mut rand::thread_rng()).unwrap()
                );
            }
        } else {
            self.message =
                format!("Sana oli \"{}\"", self.word.iter().collect::<String>());
        }
    }

    fn set_daily_word_history(&mut self, date: &NaiveDate) {
        self.daily_word_history.insert(
            *date,
            DailyWordHistory {
                word: self.word.iter().collect(),
                date: *date,
                guesses: self.guesses.iter().map(|guess| guess.iter().map(|(c, _)| *c).collect()).collect(),
                current_guess: self.current_guess,
                is_guessing: self.is_guessing,
                is_winner: self.is_winner,
            },
        );
    }

    fn set_game_statistics(&mut self) {
        self.total_played += 1;

        if self.is_winner {
            self.total_solved += 1;

            if self.game_mode != GameMode::DailyWord {
                self.streak += 1;
                if self.streak > self.max_streak {
                    self.max_streak = self.streak;
                }
            }
        } else {
            self.streak = 0;
        }
    }

    pub fn submit_guess(&mut self) -> bool {
        if !self.is_guess_allowed() {
            self.message = "Liian vähän kirjaimia!".to_owned();
            return true;
        }
        if !self.is_guess_real_word() {
            self.is_unknown = true;
            self.message = "Ei sanulistalla.".to_owned();
            return true;
        }

        self.is_reset = false;
        self.clear_message();

        self.is_winner = self.is_correct_word();
        self.submit_current_guess();
    
        if self.is_game_ended() {
            self.is_guessing = false;

            self.set_game_statistics();
            self.set_game_end_message();

            let _result = self.persist_stats();
        } else {
            self.current_guess += 1;
        }

        if self.game_mode == GameMode::DailyWord {
            let today = Local::now().naive_local().date();
            self.set_daily_word_history(&today);

            let _result = self.persist_single_daily_word(&today);
        } else {
            let _result = self.persist_game();
        }

        true
    }

    pub fn create_new_game(&mut self) -> bool {
        let next_word = if self.game_mode == GameMode::DailyWord {
            self.get_daily_word()
        } else {
            self.get_random_word()
        };

        let previous_word = mem::replace(&mut self.word, next_word);

        if previous_word.len() <= self.word_length {
            self.previous_guesses = mem::take(&mut self.guesses);
            self.previous_guesses.truncate(self.current_guess);
        } else {
            let previous_guesses = mem::take(&mut self.guesses);
            self.previous_guesses = previous_guesses
                .into_iter()
                .map(|guess| guess.into_iter().take(self.word_length).collect())
                .collect();
            self.previous_guesses.truncate(self.current_guess);
        }

        self.guesses = Vec::with_capacity(self.max_guesses);

        self.known_states = std::iter::repeat(HashMap::new())
            .take(DEFAULT_MAX_GUESSES)
            .collect::<Vec<_>>();
        self.known_at_least_counts = std::iter::repeat(HashMap::new())
            .take(DEFAULT_MAX_GUESSES)
            .collect::<Vec<_>>();
        self.discovered_characters = HashSet::new();

        if previous_word.len() == self.word_length
            && self.is_winner
            && self.game_mode == GameMode::Relay
        {
            let empty_guesses = std::iter::repeat(Vec::with_capacity(self.word_length))
                .take(self.max_guesses - 1)
                .collect::<Vec<_>>();

            self.guesses.push(previous_word.iter().map(|c| (*c, TileState::Unknown)).collect());
            self.guesses.extend(empty_guesses);

            self.current_guess = 0;
            self.submit_current_guess();
            self.current_guess = 1;
        } else {
            self.guesses = std::iter::repeat(Vec::with_capacity(self.word_length))
                .take(self.max_guesses)
                .collect::<Vec<_>>();
            self.current_guess = 0;
        }

        self.is_guessing = true;
        self.is_winner = false;
        self.is_reset = true;
        
        self.clear_message();

        if self.game_mode == GameMode::DailyWord {
            let today = Local::now().naive_local().date();
            if let Some(solve) = self.daily_word_history.get(&today).cloned() {
                for (guess_index, guess) in solve.guesses.iter().enumerate() {
                    self.guesses[guess_index] = guess.iter().map(|c| (*c, TileState::Unknown)).collect();
                    self.current_guess = guess_index;
                    self.submit_current_guess();
                }
                self.is_winner = solve.is_winner;
                self.is_guessing = solve.is_guessing;
                self.current_guess = solve.current_guess;
            }

            if !self.is_guessing {
                self.message = "Uusi sanuli huomenna!".to_owned();
            }
        } else {
            let _result = self.persist_game();
        }

        true
    }

    pub fn change_word_length(&mut self, new_length: usize) {
        self.word_length = new_length;
        self.word_list = parse_words(WORDS, self.word_length);
        // TODO: Store streaks for every word length separately
        self.streak = 0;
        self.is_menu_visible = false;

        if self.game_mode == GameMode::DailyWord {
            self.game_mode = GameMode::Classic;
        }
    }

    pub fn change_game_mode(&mut self, new_mode: GameMode) {
        self.previous_game_mode = std::mem::replace(&mut self.game_mode, new_mode);
        self.is_menu_visible = false;
        self.message = EMPTY.to_string();
        let _result = self.persist_settings();

        if self.game_mode == GameMode::DailyWord {
            self.word_list = parse_words(WORDS, 5);
            self.word_length = 5;
        }
    }
}
