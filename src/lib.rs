#![allow(warnings)]

#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate diesel;
#[macro_use]
extern crate lazy_static;

use actix::prelude::*;
use actix::{Actor, SyncContext};
use actix_cors::Cors;
use actix_files as fs;
use actix_web::{
    middleware::{Compress, Logger},
    web::{delete, get, post, put, patch, resource, route, scope},
    App, HttpResponse, HttpServer,
};

use diesel::pg::PgConnection;
use diesel::r2d2::{ConnectionManager, Pool};

// #[macro_use]
// pub mod macros;

pub mod api;
pub mod errors;
pub mod schema;
pub mod util;
pub mod view;
pub mod bot;
pub mod db;

// some type alias
pub type PoolConn = Pool<ConnectionManager<PgConnection>>;
pub type PooledConn = r2d2::PooledConnection<ConnectionManager<PgConnection>>;

// This is db executor actor
pub struct Dba(pub Pool<ConnectionManager<PgConnection>>);

impl Actor for Dba {
    type Context = SyncContext<Self>;
}

pub type DbAddr = Addr<Dba>;

pub fn init_dba() -> DbAddr {
    let db_url = dotenv::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    let cpu_num = num_cpus::get();
    let pool_num = std::cmp::max(10, cpu_num * 2 + 1) as u32;
    // p_num subject to c_num??
    let conn = Pool::builder()
        .max_size(pool_num)
        .build(manager)
        .expect("Failed to create pool.");

    SyncArbiter::start(cpu_num * 2 + 1, move || Dba(conn.clone()))
}

pub fn init_fern_logger() -> Result<(), fern::InitError> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{},{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.level(),
                record.target(),
                record.line().unwrap_or(0),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(fern::log_file("srv.log")?)
        .apply()?;

    Ok(())
}

//#[actix_rt::main]
pub async fn init_server() -> std::io::Result<()> {
    // init logger
    init_fern_logger().unwrap_or_default();
    
    // new runtime manual
    //let sys = actix_rt::System::new("server");
    
    // init actor
    let addr: DbAddr = init_dba();

    let bind_host =
        dotenv::var("BIND_ADDRESS").unwrap_or("127.0.0.1:8085".to_string());
    // config Server, App, AppState, middleware, service
    HttpServer::new(move || {
        App::new()
            .data(addr.clone())
            .wrap(Logger::default())
            .wrap(Compress::default())
            .wrap(Cors::default())
            // everything under '/api/' route
            .service(scope("/api")
                // to auth
                .service(
                    resource("/signin")
                        .route(post().to(api::auth::signin))
                )
                // to register
                .service(
                    resource("/signup")
                        .route(post().to(api::auth::signup))
                )
                .service(
                    resource("/reset")   // reset-1: request rest psw, send mail
                        .route(post().to(api::auth::reset_psw_req))
                )
                .service(
                    resource("/reset/{token}")   // reset-2: copy token, new psw
                        .route(post().to(api::auth::reset_psw))
                )
                .service(
                    resource("/users/{uname}")
                        .route(get().to(api::auth::get))
                        .route(post().to(api::auth::update))
                        .route(put().to(api::auth::change_psw))
                )
                .service(
                    resource("/blogs")
                        .route(post().to(api::blog::new))
                        .route(put().to(api::blog::update))
                        // get_list: ?per=topic&kw=&perpage=20&page=p
                        .route(get().to(api::blog::get_list)) 
                )
                .service(
                    resource("/blogs/{id}")
                        .route(get().to(api::blog::get))
                        .route(put().to(api::blog::toggle_top))
                        .route(delete().to(api::blog::del))
                )
                .service(
                    resource("/items")
                        .route(post().to(api::item::new))
                        .route(put().to(api::item::update))
                )
                .service(
                    resource("/spider")
                        .route(put().to(api::item::spider))
                )
                .service(
                    resource("/getitems/{pper}")
                        // get_list: ?per=topic&kw=&perpage=20&page=p
                        .route(get().to(api::item::get_list)) 
                )
                .service(
                    resource("/items/{slug}")
                        .route(get().to(api::item::get))
                        .route(patch().to(api::item::toggle_top))
                        // vote or veto: ?action=vote|veto
                        .route(put().to(api::item::vote_or_veto))
                        .route(delete().to(api::item::del))
                )
                .service(
                    resource("/generate-sitemap")
                        .route(get().to(view::tmpl::gen_sitemap))
                )
                .service(
                    resource("/generate-staticsite")
                        .route(get().to(view::tmpl::statify_site))
                )
                // .service(
                //     resource("/stfile/{p}")
                //         .route(delete().to(view::tmpl::del_static_file))
                // )
                .service(
                    resource("/generate-staticsite-noexpose")  // do not expose!!
                        .route(get().to(view::tmpl::statify_site_))
                )
                .default_service(route().to(|| HttpResponse::NotFound()))
            )
            .service(
                fs::Files::new("/static", "./static/") // static files
                    .default_handler(route().to(|| HttpResponse::NotFound()))
            )
            .service(
                resource("/confirm/{token}")
                    .route(get().to(api::auth::confirm_email))
            )
            .service(
                resource("/index")
                    .route(get().to(view::tmpl::dyn_index))
            )
            .service(
                resource("/from")  // query: ?by=&site=&ord=
                    .route(get().to(view::tmpl::item_from))
            )
            .service(
                resource("/a/{ty}")  // special case: /index
                    .route(get().to(view::tmpl::index_either))
            )
            .service(
                resource("/all/newest")  // special
                    .route(get().to(view::tmpl::index_newest))
            )
            .service( 
                resource("/t/{topic}/{ty}")
                    .route(get().to(view::tmpl::topic_either))
            )
            .service(
                resource("/a/{ty}/dyn")
                    .route(get().to(view::tmpl::index_dyn))
            )
            .service( 
                resource("/t/{topic}/{ty}/dyn")
                    .route(get().to(view::tmpl::topic_dyn))
            )
            .service( 
                resource("/more/{topic}/{ty}") // ?page=&perpage=42
                    .route(get().to(view::tmpl::more_item))
            )
            .service( 
                resource("/item/{slug}")
                    .route(get().to(view::tmpl::item_view))
            )
            .service(
                resource("/@{uname}")
                    .route(get().to(view::tmpl::profile))
            )
            .service(
                resource("/site/{name}")
                    .route(get().to(view::tmpl::site))
            )
            .service(
                resource("/auth") // query: ?to=
                    .route(get().to(view::form::auth_form))
            )
            .service(
                fs::Files::new("/", "./www/") // for robots.txt, sitemap
                    .index_file("all-index.html")
                    .default_handler(route().to(view::tmpl::dyn_index))
            )
            .default_service(route().to(|| HttpResponse::NotFound()))
    })
    .bind(&bind_host)
    .expect("Can not bind to host")
    .run()
    .await;

    println!("Starting http server: {}", bind_host);

    // start runtime manual
    //sys.run()

    Ok(())
}
