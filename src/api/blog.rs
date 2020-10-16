
//use futures::{Future};
use actix::{Handler, Message};
use actix_web::{
    web::{Data, Json, Path, Query},
    Error, HttpResponse, ResponseError,
    Result,
};
use base64::decode;
use diesel::prelude::*;
use diesel::{self, ExpressionMethods, QueryDsl, RunQueryDsl};

use crate::errors::{ServiceError, ServiceResult};
use crate::api::{ReqQuery};
use crate::api::auth::{CheckUser, CheckCan};
use crate::{Dba, DbAddr, PooledConn};
use crate::schema::{blogs};

// POST: /api/blogs
// 
pub async fn new(
    blog: Json<NewBlog>,
    _can: CheckCan,
    db: Data<DbAddr>,
) -> ServiceResult<HttpResponse> {
    let res = db.send(blog.into_inner()).await?;
    match res {
        Ok(b) => Ok(HttpResponse::Ok().json(b)),
        Err(err) => Ok(err.error_response()),
    }
}

impl Handler<NewBlog> for Dba {
    type Result = ServiceResult<Blog>;

    fn handle(&mut self, nb: NewBlog, _: &mut Self::Context) -> Self::Result {
        let conn: &PooledConn = &self.0.get()?;
        nb.new(conn)
    }
}

// PUT: /api/blogs
// 
pub async fn update(
    blog: Json<UpdateBlog>,
    _can: CheckCan,
    db: Data<DbAddr>,
) -> ServiceResult<HttpResponse> {
    let res = db.send(blog.into_inner()).await?;
    match res {
        Ok(b) => Ok(HttpResponse::Ok().json(b)),
        Err(err) => Ok(err.error_response()),
    }
}

impl Handler<UpdateBlog> for Dba {
    type Result = ServiceResult<Blog>;

    fn handle(&mut self, b: UpdateBlog, _: &mut Self::Context) -> Self::Result {
        let conn: &PooledConn = &self.0.get()?;
        b.update(conn)
    }
}

// GET: /api/blogs/{id}
// 
pub async fn get(
    qb: Path<i32>,
    db: Data<DbAddr>,
) -> ServiceResult<HttpResponse> {
    let blog = QueryBlog{
        id: qb.into_inner(), 
        method: String::from("GET"),
        uname: String::new()
    };
    let res = db.send(blog).await?;
    match res {
        Ok(b) => Ok(HttpResponse::Ok().json(b)),
        Err(err) => Ok(err.error_response()),
    }
}

// PUT: /api/blogs/{id}
// 
pub async fn toggle_top(
    qb: Path<i32>,
    auth: CheckCan,
    db: Data<DbAddr>,
) -> ServiceResult<HttpResponse> {
    let blog = QueryBlog{
        id: qb.into_inner(), 
        method: String::from("PUT"),
        uname: auth.uname
    };
    let res = db.send(blog).await?;
    match res {
        Ok(b) => Ok(HttpResponse::Ok().json(b.is_top)),
        Err(err) => Ok(err.error_response()),
    }
}

// DELETE: /api/blogs/{id}
// 
pub async fn del(
    qb: Path<i32>,
    auth: CheckCan,
    db: Data<DbAddr>,
) -> ServiceResult<HttpResponse> {
    let blog = QueryBlog{
        id: qb.into_inner(), 
        method: String::from("DELETE"),
        uname: auth.uname
    };
    let res = db.send(blog).await?;
    match res {
        Ok(b) => Ok(HttpResponse::Ok().json(b.aname)),
        Err(err) => Ok(err.error_response()),
    }
}

impl Handler<QueryBlog> for Dba {
    type Result = ServiceResult<Blog>;

    fn handle(&mut self, qb: QueryBlog, _: &mut Self::Context) -> Self::Result {
        let conn: &PooledConn = &self.0.get()?;
        let method: &str = &qb.method.trim();

        match method {
            "GET" => { qb.get(conn) }
            "PUT" => { qb.toggle_top(conn) }
            "DELETE" => { qb.del(conn) }
            _ => { qb.get(conn) },
        }
    }
}

// GET: api/blogs?per=topic&kw=&page=p&perpage=42
// 
pub async fn get_list(
    pq: Query<ReqQuery>,
    db: Data<DbAddr>,
) -> ServiceResult<HttpResponse> {
    let perpage = pq.perpage;
    let page = pq.page;
    let kw = pq.clone().kw;
    let per = pq.per.trim();
    let blog = match per {
        "topic" => QueryBlogs::Topic(kw, perpage, page),
        "top" => QueryBlogs::Top(kw, perpage, page),
        _ => QueryBlogs::Index(kw, perpage, page),
    };
    let res = db.send(blog).await?;
    match res {
        Ok(b) => Ok(HttpResponse::Ok().json(b)),
        Err(err) => Ok(err.error_response()),
    }
}

impl Handler<QueryBlogs> for Dba {
    type Result = ServiceResult<(Vec<Blog>, i64)>;

    fn handle(&mut self, qbs: QueryBlogs, _: &mut Self::Context) -> Self::Result {
        let conn: &PooledConn = &self.0.get()?;
        qbs.get(conn)
    }
}


// =================================================================================
// =================================================================================
// Model
// =================================================================================

#[derive(Clone, Debug, Serialize, Deserialize, Default, Identifiable, Queryable)]
#[table_name = "blogs"]
pub struct Blog {
    pub id: i32,
    pub aname: String, // unique, person's name
    pub avatar: String,
    pub intro: String,
    pub topic: String,
    pub blog_link: String,
    pub blog_host: String,
    pub tw_link: String,
    pub gh_link: String,
    pub other_link: String,
    pub is_top: bool,
    pub karma: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Insertable)]
#[table_name = "blogs"]
pub struct NewBlog {
    pub aname: String,
    pub avatar: String,
    pub intro: String,
    pub topic: String,
    pub blog_link: String,
    pub blog_host: String,
    pub gh_link: String,
    pub other_link: String,
    pub is_top: bool,
}

impl NewBlog {
    fn new(
        &self, 
        conn: &PooledConn,
    ) -> ServiceResult<Blog> {
        use crate::schema::blogs::dsl::{blogs, aname};
        let blog_name = self.aname.trim();
        let new_blog = NewBlog {
            aname: blog_name.to_owned(),
            avatar: self.avatar.trim().to_owned(),
            intro: self.intro.trim().to_owned(),
            topic: self.topic.trim().to_owned(),
            blog_link: self.blog_link.trim().to_owned(),
            blog_host: self.blog_host.trim().to_owned(),
            gh_link: self.gh_link.trim().to_owned(),
            other_link: self.other_link.trim().to_owned(),
            is_top: self.is_top,
        };
        let try_save_new_blog = diesel::insert_into(blogs)
            .values(self)
            .on_conflict_do_nothing()
            .get_result::<Blog>(conn);

        let blog_new = if let Ok(blg) = try_save_new_blog {
                blg
        } else {
            blogs.filter(aname.eq(blog_name))
                .get_result::<Blog>(conn)?
        };

        Ok(blog_new)
    }

    pub fn save_name_as_blog(
        name: &str,
        conn: &PooledConn,
    ) -> ServiceResult<Blog> {
        let new_blog = NewBlog {
            aname: name.trim().to_owned(),
            is_top: false,
            ..NewBlog::default()
        };
        new_blog.new(conn)
    }
}

impl Message for NewBlog {
    type Result = ServiceResult<Blog>;
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, AsChangeset)]
#[table_name = "blogs"]
pub struct UpdateBlog {
    pub id: i32,
    pub aname: String,
    pub avatar: String,
    pub intro: String,
    pub topic: String,
    pub blog_link: String,
    pub blog_host: String,
    pub gh_link: String,
    pub other_link: String,
    pub is_top: bool,
}

impl UpdateBlog {
    fn update(
        mut self,
        conn: &PooledConn,
    ) -> ServiceResult<Blog> {
        use crate::schema::blogs::dsl::*;
        let old = blogs.filter(id.eq(self.id))
            .get_result::<Blog>(conn)?;
        // check if anything chenged
        let new_aname = self.aname.trim();
        let new_avatar = self.avatar.trim();
        let new_intro =  self.intro.trim();
        let new_topic = self.topic.trim();
        let new_blog_link = self.blog_link.trim();
        let new_blog_host = self.blog_host.trim();
        let new_gh_link = self.gh_link.trim();
        let new_other_link = self.other_link.trim();
        let new_is_top = self.is_top;

        let check_changed: bool = new_aname != old.aname.trim()
            || new_avatar != old.avatar.trim()
            || new_intro != old.intro.trim()
            || new_topic != old.topic.trim()
            || new_blog_link != old.blog_link.trim()
            || new_gh_link != old.gh_link.trim()
            || new_other_link != old.other_link.trim()
            || new_is_top != old.is_top;
        if !check_changed {
            return Err(ServiceError::BadRequest("Nothing Changed".to_owned()));
        }

        // update item's author if aname chenged
        if new_aname != old.aname.trim() && new_aname != "" {
            use crate::api::item::Item;
            use crate::schema::items::dsl::{items, author};
            diesel::update(
                items.filter(author.eq(old.aname.trim()))
            )
            .set(author.eq(new_aname))
            .execute(conn)?;
        }

        let up = UpdateBlog {
            id: self.id,
            aname: new_aname.to_owned(),
            avatar: new_avatar.to_owned(),
            intro: new_intro.to_owned(),
            topic: new_topic.to_owned(),
            blog_link: new_blog_link.to_owned(),
            blog_host: new_blog_host.to_owned(),
            gh_link: new_gh_link.to_owned(),
            other_link: new_other_link.to_owned(),
            is_top: new_is_top,
        };

        let blog_update = diesel::update(&old).set(&up).get_result::<Blog>(conn)?;

        Ok(blog_update)
    }
}

impl Message for UpdateBlog {
    type Result = ServiceResult<Blog>;
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct QueryBlog {
    pub id: i32,
    pub method: String, // get|delete
    pub uname: String,
}

impl QueryBlog {
    fn get(
        &self, 
        conn: &PooledConn,
    ) -> ServiceResult<Blog> {
        use crate::schema::blogs::dsl::{blogs, id};
        let blog = blogs.filter(id.eq(self.id)).get_result::<Blog>(conn)?;
        Ok(blog)
    }

    fn toggle_top(
        &self, 
        conn: &PooledConn,
    ) -> ServiceResult<Blog> {
        use crate::schema::blogs::dsl::{blogs, id, is_top};
        let old = blogs
            .filter(id.eq(&self.id))
            .get_result::<Blog>(conn)?;
        let check_top: bool = old.is_top;
        let blog = diesel::update(&old)
            .set(is_top.eq(!check_top))
            .get_result::<Blog>(conn)?;

        Ok(blog)
    }

    fn del(
        &self, 
        conn: &PooledConn,
    ) -> ServiceResult<Blog> {
        use crate::schema::blogs::dsl::{blogs, id};
        // // check permission
        // let admin_env = dotenv::var("ADMIN").unwrap_or("".to_string());
        // let check_permission: bool = self.uname == admin_env;
        // if !check_permission {
        //     return Err(ServiceError::Unauthorized);
        // }

        diesel::delete(blogs.filter(id.eq(self.id))).execute(conn)?;
        Ok(Blog::default())
    }
}

impl Message for QueryBlog {
    type Result = ServiceResult<Blog>;
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub enum QueryBlogs {
    Index(String, i32, i32),
    Topic(String, i32, i32),
    Top(String, i32, i32),  // topic, perpage-42, page
    Name(String, i32, i32),
}

impl QueryBlogs {
    pub fn get(
        self, 
        conn: &PooledConn,
    ) -> ServiceResult<(Vec<Blog>, i64)> {
        use crate::schema::blogs::dsl::*;
        let mut blog_list: Vec<Blog> = Vec::new();
        let mut blog_count = 0;  // currently no need
        match self {
            QueryBlogs::Topic(t, o, p) => {
                let query = blogs.filter(topic.eq(t));
                let p_o = std::cmp::max(0, p-1);
                //blog_count = query.clone().count().get_result(conn)?;
                blog_list = query
                    .order(karma.desc())
                    .limit(o.into())
                    .offset((o * p_o).into())
                    .load::<Blog>(conn)?;
            }
            QueryBlogs::Top(t, o, p) => {
                let query = blogs.filter(is_top.eq(true)).filter(topic.eq(t));
                let p_o = std::cmp::max(0, p-1);
                //blog_count = query.clone().count().get_result(conn)?;
                blog_list = query
                    .order(karma.desc())
                    .limit(o.into())
                    .offset((o * p_o).into())
                    .load::<Blog>(conn)?;
            }
            QueryBlogs::Name(n, o, p) => {
                let query = blogs.filter(aname.eq(n));
                let p_o = std::cmp::max(0, p-1);
                //blog_count = query.clone().count().get_result(conn)?;
                blog_list = query
                    .order(karma.desc())
                    .limit(o.into())
                    .offset((o * p_o).into())
                    .load::<Blog>(conn)?;
            }
            _ => {
                blog_list = blogs
                    .filter(is_top.eq(true))
                    .order(karma.desc()).limit(42).load::<Blog>(conn)?;
                //blog_count = blog_list.len() as i64;
            }
        }
        Ok((blog_list, blog_count))
    }
}

impl Message for QueryBlogs {
    type Result = ServiceResult<(Vec<Blog>, i64)>;
}

// TODO
//
//#[derive(Clone, Debug, Serialize, Deserialize, Default, Identifiable, Queryable)]
//#[table_name = "pkgs"]
pub struct Pkg {
    pub id: i32,
    pub pname: String,
    pub slug: String,     // uri friendly
    pub lang: String,     // programing lang: Rust|Python...
    pub domain: String,   // web|game|renderer|parser|Application ...
    pub sub0: String,     // framework|io|...
    pub sub1: String,     // reserve
    pub intro: String,
    pub link: String, 
    pub logo: String,
    pub vote: i32,
}

// TODO
// How x do y
/*
x_id: topic, eg. Rust
y_id: topic, eg. Web
stack:  as what in Tech Stack, eg. webframework
app: what, eg. Actix-web
*/

//#[derive(Clone, Debug, Serialize, Deserialize, Default, Identifiable, Queryable)]
//#[table_name = "topics"]
pub struct Topic {
    pub id: i32,
    pub tname: String,
    pub slug: String,    // uri friendly
    pub ty: String,      // Programming|Company|Tech|Culture ...
    pub intro: String,
    pub logo: String,
    pub vote: i32,
}

//#[derive(Clone, Debug, Serialize, Deserialize, Default, Identifiable, Queryable)]
//#[table_name = "stacks"]
pub struct Stack {
    pub id: i32,
    pub sname: String,
    pub slug: String,    // uri friendly
    pub intro: String,
    pub logo: String,
    pub vote: i32,
}

// #[table_name = "stackpkg"]
pub struct StackPkg {
    pub stack_id: i32,
    pub pkg_id: i32,
    pub ty: String,
}
