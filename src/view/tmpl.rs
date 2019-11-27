
use futures::{Future, future::result};
use actix::{Handler, Message};
use crate::errors::{ServiceError, ServiceResult};
use crate::api::auth::{verify_token, CheckUser};
use crate::api::item::{Item, QueryItems};
use crate::api::blog::{Blog, QueryBlogs};
use crate::view::TEMPLATE as tmpl;
use crate::{Dba, DbAddr, PooledConn};
use actix_http::http;
use actix_web::{
    web::{Data, Path, Query},
    Error, HttpResponse, ResponseError,
};
use chrono::{SecondsFormat, Utc};

// GET /
//
pub fn index() -> Result<HttpResponse, Error> {
    let res = String::from_utf8(
        std::fs::read("www/index.html")
            .unwrap_or("Not Found".to_owned().into_bytes()), // handle not found
    )
    .unwrap_or_default();
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(res))
}

// GET /{ty} // special: /index, /Misc
//
// response dynamically
pub fn index_dyn(
    db: Data<DbAddr>,
    p: Path<String>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let home_msg = Topic { 
        topic: String::from("all"), 
        ty: p.into_inner(),
        page: 1, 
    };
    
    db.send(home_msg).from_err().and_then(|res| match res {
        Ok(msg) => {
            let mut ctx = tera::Context::new();
            ctx.insert("items", &msg.items);
            ctx.insert("blogs", &msg.blogs);

            let mesg: Vec<&str> = (&msg.message).split("-").collect();
            let typ = mesg[1];
            ctx.insert("ty", &typ);
            ctx.insert("topic", "all");

            let h = tmpl.render("home.html", &ctx).map_err(|_| {
                ServiceError::InternalServerError("template failed".into())
            })?;
            let dir = "www/".to_owned() + &msg.message + ".html";
            std::fs::write(dir, h.as_bytes())?;
            Ok(HttpResponse::Ok().content_type("text/html").body(h))
        }
        Err(e) => Ok(e.error_response()),
    })
}

// GET /t/{topic}/{ty}
//
pub fn topic(
    db: Data<DbAddr>,
    p: Path<(String, String)>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let pa = p.into_inner();
    let topic = pa.0;
    let ty = pa.1;

    let topic_msg = Topic{ topic, ty, page: 1 };
    result(
        topic_msg.validate()
    )
    .from_err()
    .and_then(move |_| db.send(topic_msg).from_err())
    .and_then(|res| match res {
        Ok(msg) => {
            let mut ctx = tera::Context::new();
            ctx.insert("items", &msg.items);
            ctx.insert("blogs", &msg.blogs);
            let mesg: Vec<&str> = (&msg.message).split("-").collect();
            let tpc = mesg[0];
            let typ = mesg[1];
            ctx.insert("ty", typ);
            ctx.insert("topic", tpc);

            let h = tmpl.render("home.html", &ctx).map_err(|_| {
                ServiceError::InternalServerError("template failed".into())
            })?;
            let t_dir = "www/".to_owned() + &msg.message + ".html";
            std::fs::write(&t_dir, h.as_bytes())?;
            Ok(HttpResponse::Ok().content_type("text/html").body(h))
        }
        Err(e) => Ok(e.error_response()),
    })
}

// GET /more/{topic}/{ty}?page=&perpage=42
//
pub fn more_item(
    db: Data<DbAddr>,
    p: Path<(String, String)>,
    pq: Query<PageQuery>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let pa = p.into_inner();
    let topic = pa.0;
    let ty = pa.1;
    // extract Query
    let page = std::cmp::max(pq.page, 1);
    let perpage = pq.clone().perpage;

    let topic_msg = Topic{ topic, ty, page };
    result(
        topic_msg.validate()
    )
    .from_err()
    .and_then(move |_| db.send(topic_msg).from_err())
    .and_then(|res| match res {
        Ok(msg) => {
            let mut ctx = tera::Context::new();
            ctx.insert("items", &msg.items);

            let h = tmpl.render("more_item.html", &ctx).map_err(|_| {
                ServiceError::InternalServerError("template failed".into())
            })?;
            Ok(HttpResponse::Ok().content_type("text/html").body(h))
        }
        Err(e) => Ok(e.error_response()),
    })
}

// GET /me/index.html // spa
// try_uri for spa
// 
pub fn spa_index() -> Result<HttpResponse, Error> {
    let res = String::from_utf8(
        std::fs::read("spa/index.html")
            .unwrap_or("Not Found".to_owned().into_bytes()),
    )
    .unwrap_or_default();
    Ok(HttpResponse::build(http::StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(res))
}

// =====================================================================
// type model
// =====================================================================

// for extrct query param
#[derive(Deserialize, Clone)]
pub struct PageQuery {
    page: i32,
    perpage: i32,
}

// result struct in response
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ItemBlogMsg {
    pub status: i32,
    pub message: String,
    pub items: Vec<Item>,
    pub blogs: Vec<Blog>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Topic{
    pub topic: String,
    pub ty: String,
    pub page: i32,
}

impl Topic {
    fn validate(&self) -> ServiceResult<()> {
        let tp: &str = &self.topic.trim();
        let ty: &str = &self.ty.trim();
        let page = &self.page;
    
        let check = ty == "index" 
        || ty == "Article" 
        || ty == "Book" 
        || ty == "Event" 
        || ty == "Podcast" 
        || ty == "Translate"
        || ty == "Misc";

        if check {
            Ok(())
        } else {
            Err(ServiceError::BadRequest("Invalid Input".into()))
        }
    }
}

impl Message for Topic {
    type Result = ServiceResult<ItemBlogMsg>;
}

// per topic and ty
impl Handler<Topic> for Dba {
    type Result = ServiceResult<ItemBlogMsg>;

    fn handle(
        &mut self,
        t: Topic,
        _: &mut Self::Context,
    ) -> Self::Result {
        use crate::schema::items::dsl::*;
        use crate::schema::blogs::dsl::{blogs};
        let conn = &self.0.get()?;
        let tpc = t.topic;
        let typ = t.ty;

        let tp = tpc.trim().to_lowercase();

        let (query_item, query_blog) = if tp == "all" {
            (
                QueryItems::Index(typ.clone(), 42, t.page),
                QueryBlogs::Index("index".into(), 42, 1)
            )
        } else {
            (
                QueryItems::Tt(tpc.clone(), typ.clone(), 42, t.page),
                QueryBlogs::Topic(tpc.clone(), 42, 1)
            )
        };

        let (i_list, _) = query_item.get(conn)?;
        let (b_list, _) = query_blog.get(conn)?;

        Ok(ItemBlogMsg {
            status: 201,
            message: tpc + "-" + &typ, // send back the ty and topic info
            items: i_list,
            blogs: b_list,
        })
    }
}
