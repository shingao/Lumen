//! KeyValues de Valve (VDF, ACF) : `"clé" "valeur"` ou `"clé" { … }`, commentaires `//`,
//! conditions `[$WIN32]` ignorées. Lecture tolérante : un fichier tronqué ou mal formé donne une
//! erreur, jamais de plantage ; les clés se lisent sans tenir compte de la casse.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    Obj(Vec<(String, Value)>),
}

impl Value {
    /// Premier enfant de ce nom (casse ignorée).
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Obj(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            Value::Str(_) => None,
        }
    }

    /// Chemin de clés, ex. `["AppState", "name"]`.
    pub fn path(&self, keys: &[&str]) -> Option<&Value> {
        keys.iter().try_fold(self, |v, k| v.get(k))
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Obj(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Obj(items) => items,
            Value::Str(_) => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VdfError {
    /// Fin du fichier au milieu d'un bloc ou d'une chaîne (fichier tronqué).
    Truncated,
    /// Accolade fermante en trop, ou clé sans valeur.
    Malformed,
    /// Imbrication trop profonde (fichier hostile ou corrompu).
    TooDeep,
}

const MAX_DEPTH: usize = 64;

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Text(String),
    Open,
    Close,
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Lexer<'_> {
    fn next(&mut self) -> Result<Option<Token>, VdfError> {
        loop {
            match self.chars.peek().copied() {
                None => return Ok(None),
                Some(c) if c.is_whitespace() => {
                    self.chars.next();
                }
                Some('/') => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'/') {
                        for c in self.chars.by_ref() {
                            if c == '\n' {
                                break;
                            }
                        }
                    } else {
                        return Ok(Some(Token::Text(self.bare('/'))));
                    }
                }
                Some('[') => {
                    // Condition de plateforme : ignorée.
                    for c in self.chars.by_ref() {
                        if c == ']' {
                            break;
                        }
                    }
                }
                Some('{') => {
                    self.chars.next();
                    return Ok(Some(Token::Open));
                }
                Some('}') => {
                    self.chars.next();
                    return Ok(Some(Token::Close));
                }
                Some('"') => {
                    self.chars.next();
                    let mut s = String::new();
                    loop {
                        match self.chars.next() {
                            None => return Err(VdfError::Truncated),
                            Some('"') => break,
                            Some('\\') => match self.chars.next() {
                                None => return Err(VdfError::Truncated),
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some(c) => s.push(c),
                            },
                            Some(c) => s.push(c),
                        }
                    }
                    return Ok(Some(Token::Text(s)));
                }
                Some(c) => {
                    self.chars.next();
                    return Ok(Some(Token::Text(self.bare(c))));
                }
            }
        }
    }

    fn bare(&mut self, first: char) -> String {
        let mut s = String::from(first);
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                break;
            }
            s.push(c);
            self.chars.next();
        }
        s
    }
}

fn block(lex: &mut Lexer<'_>, depth: usize, top: bool) -> Result<Vec<(String, Value)>, VdfError> {
    if depth > MAX_DEPTH {
        return Err(VdfError::TooDeep);
    }
    let mut items = Vec::new();
    loop {
        let key = match lex.next()? {
            None if top => return Ok(items),
            None => return Err(VdfError::Truncated),
            Some(Token::Close) if !top => return Ok(items),
            Some(Token::Close) | Some(Token::Open) => return Err(VdfError::Malformed),
            Some(Token::Text(k)) => k,
        };
        let value = match lex.next()? {
            None => return Err(VdfError::Truncated),
            Some(Token::Text(v)) => Value::Str(v),
            Some(Token::Open) => Value::Obj(block(lex, depth + 1, false)?),
            Some(Token::Close) => return Err(VdfError::Malformed),
        };
        items.push((key, value));
    }
}

/// Document VDF entier (un objet racine qui contient les blocs de premier niveau).
pub fn parse(text: &str) -> Result<Value, VdfError> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lex = Lexer { chars: text.chars().peekable() };
    Ok(Value::Obj(block(&mut lex, 0, true)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifeste_steam() {
        let doc = parse(
            "\u{feff}\"AppState\"\n{\n\t\"appid\"\t\t\"570\"\n\t\"name\"\t\t\"Dota 2\"\n\t// commentaire\n\t\"UserConfig\"\n\t{\n\t\t\"language\"\t\t\"french\"\n\t}\n}\n",
        )
        .unwrap();
        assert_eq!(doc.path(&["appstate", "NAME"]).and_then(Value::str), Some("Dota 2"));
        assert_eq!(doc.path(&["AppState", "UserConfig", "language"]).and_then(Value::str), Some("french"));
    }

    #[test]
    fn echappements_conditions_et_mots_nus() {
        let doc = parse(r#""libraryfolders" { "0" { "path" "D:\\SteamLibrary" "label" "Jeux \"rapides\"" } [$WIN32] bare value }"#).unwrap();
        assert_eq!(doc.path(&["libraryfolders", "0", "path"]).and_then(Value::str), Some("D:\\SteamLibrary"));
        assert_eq!(doc.path(&["libraryfolders", "0", "label"]).and_then(Value::str), Some("Jeux \"rapides\""));
        assert_eq!(doc.path(&["libraryfolders", "bare"]).and_then(Value::str), Some("value"));
    }

    #[test]
    fn fichiers_corrompus() {
        assert_eq!(parse("\"AppState\" { \"appid\" \"570\""), Err(VdfError::Truncated));
        assert_eq!(parse("\"AppState\" { \"name\" \"Dota"), Err(VdfError::Truncated));
        assert_eq!(parse("\"a\" \"b\" }"), Err(VdfError::Malformed));
        assert_eq!(parse("\"a\" { \"b\" }"), Err(VdfError::Malformed));
        assert_eq!(parse(&"\"a\" {".repeat(100)), Err(VdfError::TooDeep));
        // Octets invalides remplacés à la lecture : la structure reste lisible.
        assert!(parse("\"a\" \"\u{fffd}\u{fffd}\"").is_ok());
        assert_eq!(parse("").map(|v| v.entries().len()), Ok(0));
    }
}
