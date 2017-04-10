use distributary::srv;
use distributary::DataType;
use tarpc;
use tarpc::util::FirstSocketAddr;
use tarpc::future::client::{ClientExt, Options};
use tokio_core::reactor;

use common::{Writer, Reader, ArticleResult, Period};

const ARTICLE_NODE: usize = 1;
const VOTE_NODE: usize = 2;
const END_NODE: usize = 4;

pub struct C(srv::ext::FutureClient, reactor::Core);
use std::ops::{Deref, DerefMut};
impl Deref for C {
    type Target = srv::ext::FutureClient;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for C {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl C {
    pub fn insert(&mut self, view: usize, data: Vec<DataType>) {
        self.1.run(self.0.insert(view, data)).unwrap();
    }
    pub fn query(&mut self,
                 view: usize,
                 key: DataType)
                 -> Result<Vec<Vec<DataType>>, tarpc::Error<()>> {
        self.1.run(self.0.query(view, key))
    }
}
unsafe impl Send for C {}

fn mkc(addr: &str) -> C {
    use self::srv::ext::FutureClient;
    let mut core = reactor::Core::new().unwrap();
    for _ in 0..3 {
        let c = FutureClient::connect(addr.first_socket_addr(),
                                      Options::default().handle(core.handle()));
        match core.run(c) {
            Ok(client) => {
                return C(client, core);
            }
            Err(_) => {
                use std::thread;
                use std::time::Duration;
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    panic!("Failed to connect to netsoup server");
}

pub fn make_reader(addr: &str) -> C {
    mkc(addr)
}

pub fn make_writer(addr: &str) -> C {
    mkc(addr)
}

impl Writer for C {
    type Migrator = ();
    fn make_article(&mut self, article_id: i64, title: String) {
        self.insert(ARTICLE_NODE, vec![article_id.into(), title.into()]);
    }
    fn vote(&mut self, ids: &[(i64, i64)]) -> Period {
        for &(user_id, article_id) in ids {
            self.insert(VOTE_NODE, vec![user_id.into(), article_id.into()]);
        }
        Period::PreMigration
    }
}

impl Reader for C {
    fn get(&mut self, ids: &[(i64, i64)]) -> (Result<Vec<ArticleResult>, ()>, Period) {
        let res = ids.iter()
            .map(|&(_, article_id)| {
                // rustfmt
                self.query(END_NODE, article_id.into()).map_err(|_| ()).map(|rows| {
                    match rows.into_iter().next() {
                        Some(row) => {
                            match row[1] {
                                DataType::TinyText(..) |
                                DataType::Text(..) => {
                                    use std::borrow::Cow;
                                    let t: Cow<_> = (&row[1]).into();
                                    ArticleResult::Article {
                                        id: row[0].clone().into(),
                                        title: t.to_string(),
                                        votes: row[2].clone().into(),
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                        None => ArticleResult::NoSuchArticle,
                    }
                })
            })
            .collect();
        (res, Period::PreMigration)
    }
}
