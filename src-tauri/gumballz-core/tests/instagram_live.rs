//! Live check of the Instagram category with a real session. Ignored
//! by default; runs with a cookies.txt (Netscape) from instagram.com:
//!
//! ```text
//! GUMBALLZ_IG_COOKIES="$HOME/Library/Application Support/com.gumballz.get/cookies/instagram.com/_default.txt" \
//!   cargo test -p gumballz-core --test instagram_live -- --ignored --nocapture
//! ```
//!
//! It only reads (profile, posts, reels, stories, highlights, followers,
//! comments, likes, hashtag). No unfollow and no publication.

use std::sync::atomic::AtomicBool;

use gumballz_core::core::tools::instagram::{follow, media, profile, social, IgClient, Session};
use gumballz_core::core::tools::noop_progress;

#[tokio::test]
#[ignore]
async fn ig_live_read_only() {
    let path = std::env::var("GUMBALLZ_IG_COOKIES").expect("GUMBALLZ_IG_COOKIES");
    let content = std::fs::read_to_string(&path).expect("cookies.txt");
    let session = Session::from_netscape(&content).expect("sessionid + ds_user_id");
    let client = IgClient::new(session).unwrap();
    let flag = AtomicBool::new(false);
    let p = noop_progress();

    let me = profile::whoami(&client).await.expect("whoami");
    println!(
        "logged in as @{} ({} followers)",
        me.username, me.follower_count
    );

    let target = std::env::var("GUMBALLZ_IG_TARGET").unwrap_or_else(|_| "instagram".into());
    let user = profile::resolve_user(&client, &target)
        .await
        .expect("resolve_user");
    println!(
        "@{} pk={} posts={} private={}",
        user.username, user.pk, user.media_count, user.is_private
    );

    let posts = profile::user_posts(&client, &user.username, 15, &flag, &p, "t")
        .await
        .expect("user_posts (graphql)");
    println!(
        "posts: {} (first {} · {} files)",
        posts.len(),
        posts.first().map(|x| x.code.as_str()).unwrap_or("-"),
        posts.first().map(|x| x.files.len()).unwrap_or(0)
    );
    assert!(posts.len() > 12, "GraphQL pagination should exceed 12");

    let reels = profile::user_reels(&client, &user.pk, 5, &flag, &p, "t")
        .await
        .expect("user_reels");
    println!("reels: {}", reels.len());

    let tagged = profile::user_tagged(&client, &user.pk, 5, &flag, &p, "t")
        .await
        .expect("user_tagged");
    println!("tagged: {}", tagged.len());

    let hls = profile::highlights(&client, &user.pk)
        .await
        .expect("highlights");
    println!("highlights: {}", hls.len());
    if let Some(h) = hls.first() {
        let items = profile::highlight_items(&client, &h.id)
            .await
            .expect("highlight_items");
        println!("  '{}' with {} items", h.title, items.len());
    }
    let stories = profile::stories_of(&client, &user.pk)
        .await
        .expect("stories_of");
    println!("active stories: {}", stories.len());

    if let Some(post) = posts.first() {
        let item = media::post_info(&client, &post.code)
            .await
            .expect("post_info");
        println!(
            "post_info ok: {} likes, {} comments",
            item.like_count, item.comment_count
        );
        let comments = social::comments(&client, &item.pk, 30, &flag, &p, "t")
            .await
            .expect("comments");
        let (likes, likers) = social::likers(&client, &item.pk).await.expect("likers");
        println!(
            "comments: {} · likers: {} of {}",
            comments.len(),
            likers.len(),
            likes
        );
    }

    let following = follow::following(&client, &me.pk, 60, &flag, &p, "t")
        .await
        .expect("following");
    let followers = follow::followers(&client, &me.pk, 60, &flag, &p, "t")
        .await
        .expect("followers");
    let a = follow::analyze(followers, following, &Default::default());
    println!(
        "followers {} · following {} · not following back (sample) {}",
        a.followers_count,
        a.following_count,
        a.not_following_back.len()
    );

    let tag = social::tag_info(&client, "sunset").await.expect("tag_info");
    println!("#sunset: {} posts", tag.media_count);
    let tag_media = social::tag_media(&client, "sunset", "recent", 10, &flag, &p, "t")
        .await
        .unwrap_or_default();
    println!("#sunset recent: {}", tag_media.len());
}
