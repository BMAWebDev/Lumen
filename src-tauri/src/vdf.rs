//! Minimal parser for the KeyValues text format (VDF) used by Steam
//! in `libraryfolders.vdf` and `appmanifest_*.acf`.

#[derive(Debug, Clone)]
pub enum Vdf {
    Str(String),
    Obj(Vec<(String, Vdf)>),
}

impl Vdf {
    /// Looks up a key (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Obj(items) => items
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Str(s) => Some(s.as_str()),
            Vdf::Obj(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Obj(items) => items,
            Vdf::Str(_) => &[],
        }
    }
}

enum Tok {
    S(String),
    Open,
    Close,
}

pub fn parse(text: &str) -> Vdf {
    let tokens = tokenize(text);
    let mut pos = 0;
    Vdf::Obj(parse_obj(&tokens, &mut pos))
}

fn tokenize(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = it.next() {
                    match c {
                        '\\' => match it.next() {
                            Some('n') => s.push('\n'),
                            Some('t') => s.push('\t'),
                            Some(o) => s.push(o),
                            None => {}
                        },
                        '"' => break,
                        o => s.push(o),
                    }
                }
                out.push(Tok::S(s));
            }
            '{' => out.push(Tok::Open),
            '}' => out.push(Tok::Close),
            '/' if it.peek() == Some(&'/') => {
                for c in it.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                let mut s = String::from(c);
                while let Some(&n) = it.peek() {
                    if n.is_whitespace() || n == '{' || n == '}' || n == '"' {
                        break;
                    }
                    s.push(n);
                    it.next();
                }
                out.push(Tok::S(s));
            }
        }
    }
    out
}

fn parse_obj(t: &[Tok], pos: &mut usize) -> Vec<(String, Vdf)> {
    let mut items = Vec::new();
    while *pos < t.len() {
        match &t[*pos] {
            Tok::Close => {
                *pos += 1;
                break;
            }
            Tok::Open => *pos += 1,
            Tok::S(k) => {
                let key = k.clone();
                *pos += 1;
                match t.get(*pos) {
                    Some(Tok::S(v)) => {
                        items.push((key, Vdf::Str(v.clone())));
                        *pos += 1;
                    }
                    Some(Tok::Open) => {
                        *pos += 1;
                        let child = parse_obj(t, pos);
                        items.push((key, Vdf::Obj(child)));
                    }
                    _ => {}
                }
            }
        }
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_folders() {
        let txt = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps"
		{
			"228980"		"201000000"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary" // comment
	}
}"#;
        let v = parse(txt);
        let lf = v.get("libraryfolders").unwrap();
        let paths: Vec<&str> = lf.entries().iter().filter_map(|(_, e)| e.str("path")).collect();
        assert_eq!(paths, vec![r"C:\Program Files (x86)\Steam", r"D:\SteamLibrary"]);
    }

    #[test]
    fn app_manifest() {
        let txt = "\"AppState\"\n{\n\t\"appid\"\t\t\"1245620\"\n\t\"name\"\t\t\"ELDEN RING\"\n\t\"installdir\"\t\t\"ELDEN RING\"\n\t\"SizeOnDisk\"\t\t\"52000000000\"\n}";
        let v = parse(txt);
        let st = v.get("appstate").unwrap();
        assert_eq!(st.str("AppID"), Some("1245620"));
        assert_eq!(st.str("name"), Some("ELDEN RING"));
        assert_eq!(st.str("SizeOnDisk"), Some("52000000000"));
    }
}
