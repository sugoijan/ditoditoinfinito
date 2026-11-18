//! A share's upload as the server takes it, sans-IO
//! (`docs/plans/shared-songs.md`): the share made (`POST <base>/s`), each
//! of its files put (`PUT`, one a file, then the pack's art where it
//! comes), the share finished (`POST <base>/s/<id>/finish`), every answer
//! read as a refusal unless it is a success. Both shells upload through
//! it, each with its own transport (desktop's blocking HTTP and files,
//! the web's fetch and blobs): the shell sends the request [`Upload::next`]
//! says with the upload's `Authorization` and hands back the answer.

use serde::Serialize;

use super::{Created, NewShare, ShareError, file_url, grant_refusal, refusal};

/// One share's upload: what is sent next, and what came of it so far.
#[derive(Clone, Debug)]
pub struct Upload {
    base: String,
    auth: String,
    grant: bool,
    body: String,
    /// Each song's files, by the path the share keeps them at, and their
    /// bytes, in the order the share lists them.
    files: Vec<Vec<(String, f64)>>,
    art: Option<f64>,
    id: Option<String>,
    at: At,
}

/// Where the upload is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum At {
    Create,
    Put { song: usize, file: usize },
    Art,
    Finish,
    Done,
}

/// The request to send next ([`Upload::next`]), each with the upload's
/// `Authorization` ([`Upload::auth`]).
#[derive(Clone, Debug, PartialEq)]
pub enum Step<'a> {
    /// `POST` the share's JSON (`Content-Type: application/json`).
    Create { url: String, body: &'a str },
    /// `PUT` file `file` of song `song` (its bytes: the shell's), kept by
    /// the share at `path`.
    Put {
        url: String,
        song: usize,
        file: usize,
        path: &'a str,
    },
    /// `PUT` the pack's art.
    Art { url: String },
    /// `POST` nothing: the share is finished.
    Finish { url: String },
    /// The share is made: its id.
    Done(&'a str),
}

/// What an answer moved ([`Upload::answered`]): a file's bytes sent, a
/// song's files all sent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Told {
    Bytes(f64),
    Song,
}

impl Upload {
    /// The share `new` uploaded to the server at `base` with `auth`
    /// (`Bearer <token>`, or `Grant <grant>` for an online room's stage:
    /// `grant`, whose refusals keep their code), `art` the pack's art's
    /// bytes where it comes (`new.pack_art` says so).
    pub fn new<R: Serialize>(
        base: &str,
        new: &NewShare<R>,
        art: Option<f64>,
        auth: String,
        grant: bool,
    ) -> Result<Upload, ShareError> {
        let body = serde_json::to_string(new).map_err(ShareError::failed("making the share"))?;
        Ok(Upload {
            base: base.to_string(),
            auth,
            grant,
            body,
            files: new
                .songs
                .iter()
                .map(|s| s.files.iter().map(|f| (f.path.clone(), f.bytes)).collect())
                .collect(),
            art,
            id: None,
            at: At::Create,
        })
    }

    /// Every request's `Authorization`.
    pub fn auth(&self) -> &str {
        &self.auth
    }

    /// The request to send now.
    pub fn next(&self) -> Step<'_> {
        let id = self.id.as_deref().unwrap_or_default();
        match self.at {
            At::Create => Step::Create {
                url: format!("{}/s", self.base),
                body: &self.body,
            },
            At::Put { song, file } => {
                let path = &self.files[song][file].0;
                Step::Put {
                    url: file_url(&self.base, id, song, path),
                    song,
                    file,
                    path,
                }
            }
            At::Art => Step::Art {
                url: format!("{}/s/{id}/art", self.base),
            },
            At::Finish => Step::Finish {
                url: format!("{}/s/{id}/finish", self.base),
            },
            At::Done => Step::Done(id),
        }
    }

    /// The answer to [`Upload::next`]'s request: its `status` and its
    /// `body` (read where it could be); `told` hears what it moved. A
    /// refusal is the server's, a file's under its path.
    pub fn answered(
        &mut self,
        status: u16,
        body: Option<&str>,
        told: &mut dyn FnMut(Told),
    ) -> Result<(), ShareError> {
        if !(200..300).contains(&status) {
            let refused = if self.grant {
                grant_refusal(status, body)
            } else {
                refusal(status, body)
            };
            return Err(match self.at {
                At::Put { song, file } => ShareError::of(self.files[song][file].0.clone(), refused),
                _ => refused,
            });
        }
        self.at = match self.at {
            At::Create => {
                let created: Created = serde_json::from_str(body.unwrap_or_default())
                    .map_err(|e| ShareError::Answer(e.to_string()))?;
                self.id = Some(created.id);
                self.first_put(0, told)
            }
            At::Put { song, file } => {
                told(Told::Bytes(self.files[song][file].1));
                if file + 1 < self.files[song].len() {
                    At::Put {
                        song,
                        file: file + 1,
                    }
                } else {
                    told(Told::Song);
                    self.first_put(song + 1, told)
                }
            }
            At::Art => {
                told(Told::Bytes(self.art.unwrap_or_default()));
                At::Finish
            }
            At::Finish | At::Done => At::Done,
        };
        Ok(())
    }

    /// The request [`Upload::next`] said could not be sent or answered
    /// (the connection, the system): what failed, a file's under its
    /// path.
    pub fn lost(&self, error: impl std::fmt::Display) -> ShareError {
        match self.at {
            At::Create | At::Done => ShareError::failed("making the share")(error),
            At::Put { song, file } => ShareError::of(
                self.files[song][file].0.clone(),
                ShareError::failed("sending a file")(error),
            ),
            At::Art => ShareError::failed("sending a file")(error),
            At::Finish => ShareError::failed("finishing the share")(error),
        }
    }

    /// The first file to put from song `song` on (songs without files
    /// passed over, each told done), else the art, else the finish.
    fn first_put(&self, song: usize, told: &mut dyn FnMut(Told)) -> At {
        for (n, files) in self.files.iter().enumerate().skip(song) {
            if !files.is_empty() {
                return At::Put { song: n, file: 0 };
            }
            told(Told::Song);
        }
        if self.art.is_some() {
            At::Art
        } else {
            At::Finish
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::{ShareFileDoc, ShareSongDoc};

    fn new(files: &[&[(&str, f64)]], art: bool) -> NewShare<serde_json::Value> {
        NewShare {
            pack: "P".into(),
            expires: "1d".into(),
            songs: files
                .iter()
                .map(|fs| ShareSongDoc {
                    record: serde_json::json!({}),
                    files: fs
                        .iter()
                        .map(|(p, b)| ShareFileDoc {
                            path: p.to_string(),
                            bytes: *b,
                            url: String::new(),
                        })
                        .collect(),
                })
                .collect(),
            pack_art: art,
        }
    }

    /// Every request in order, each answered `ok`, and what was told.
    fn run(up: &mut Upload) -> (Vec<String>, Vec<Told>) {
        let (mut sent, mut told) = (Vec::new(), Vec::new());
        loop {
            let (url, status, body) = match up.next() {
                Step::Create { url, .. } => (url, 201, Some(r#"{"id":"Ab12"}"#)),
                Step::Put { url, .. } | Step::Art { url } | Step::Finish { url } => {
                    (url, 200, None)
                }
                Step::Done(id) => {
                    sent.push(format!("done {id}"));
                    return (sent, told);
                }
            };
            sent.push(url);
            up.answered(status, body, &mut |t| told.push(t)).unwrap();
        }
    }

    #[test]
    fn a_share_is_made_its_files_put_in_order_then_finished() {
        let share = new(
            &[&[("a.ssc", 10.0), ("a.ogg", 90.0)], &[("b.sm", 5.0)]],
            true,
        );
        let mut up =
            Upload::new("https://h/api", &share, Some(7.0), "Bearer t".into(), false).unwrap();
        assert_eq!(up.auth(), "Bearer t");
        let Step::Create { body, .. } = up.next() else {
            panic!("the share first")
        };
        assert_eq!(body, serde_json::to_string(&share).unwrap());
        let (sent, told) = run(&mut up);
        assert_eq!(
            sent,
            [
                "https://h/api/s".to_string(),
                file_url("https://h/api", "Ab12", 0, "a.ssc"),
                file_url("https://h/api", "Ab12", 0, "a.ogg"),
                file_url("https://h/api", "Ab12", 1, "b.sm"),
                "https://h/api/s/Ab12/art".into(),
                "https://h/api/s/Ab12/finish".into(),
                "done Ab12".into(),
            ]
        );
        assert_eq!(
            told,
            [
                Told::Bytes(10.0),
                Told::Bytes(90.0),
                Told::Song,
                Told::Bytes(5.0),
                Told::Song,
                Told::Bytes(7.0),
            ]
        );
    }

    #[test]
    fn refusals_are_the_servers_a_files_under_its_path() {
        let share = new(&[&[("a.ssc", 1.0)]], false);
        let mut up = Upload::new("b", &share, None, "Grant g".into(), true).unwrap();
        let none = &mut |_| {};
        assert_eq!(
            up.clone().answered(401, Some("grant_expired"), none),
            Err(grant_refusal(401, Some("grant_expired")))
        );
        assert_eq!(
            up.clone()
                .answered(201, Some("not json"), none)
                .unwrap_err(),
            ShareError::Answer(
                serde_json::from_str::<Created>("not json")
                    .unwrap_err()
                    .to_string()
            )
        );
        up.answered(201, Some(r#"{"id":"x"}"#), none).unwrap();
        let refused = up.clone().answered(413, None, none).unwrap_err();
        assert_eq!(refused, ShareError::of("a.ssc", ShareError::TooLarge));
        assert!(!crate::share::grant_expired(&refused));
        // A grant that expired part-way: the room's client renews it and
        // uploads again (`RoomClient::upload_finished`).
        let expired = up
            .clone()
            .answered(401, Some("grant_expired"), none)
            .unwrap_err();
        assert!(crate::share::grant_expired(&expired), "{expired:?}");
        assert_eq!(
            up.lost("reset"),
            ShareError::of("a.ssc", ShareError::failed("sending a file")("reset"))
        );
        up.answered(200, None, none).unwrap();
        assert_eq!(up.lost("x"), ShareError::failed("finishing the share")("x"));
        assert_eq!(
            up.clone().answered(500, Some("oops"), none),
            Err(grant_refusal(500, Some("oops")))
        );
        up.answered(200, None, none).unwrap();
        assert_eq!(up.next(), Step::Done("x"));
        // A token's: refused as a token's.
        let mut up = Upload::new("b", &share, None, "Bearer t".into(), false).unwrap();
        assert_eq!(
            up.answered(403, Some("no"), none),
            Err(ShareError::TokenRefused)
        );
    }
}
