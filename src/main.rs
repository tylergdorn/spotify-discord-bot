use std::env;

use serenity::{
    async_trait,
    model::{channel::Message, gateway::Ready, id::ChannelId, id::MessageId},
    prelude::*,
};

use rspotify::{
    client::Spotify,
    oauth2::{SpotifyClientCredentials, SpotifyOAuth},
    util::get_token_without_cache,
};

use regex::Regex;
use tokio;

struct Handler {
    spotify_refresh_token: String,
    // who know why we need this
    spotify_oauth: SpotifyOAuth,
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, ctx: Context, msg: Message) {
        println!("got a message");
        self.message_handler(msg).await;
        // let re = Regex::new(r".*open.spotify.com/track/([^\s?]*)?.*").unwrap();
        // if let Some(captures) = re.captures(msg.content.as_str()) {
        //     // Sending a message can fail, due to a network error, an
        //     // authentication error, or lack of permissions to post in the
        //     // channel, so log to stdout when some error happens, with a
        //     // description of it.
        //     println!("{}", captures.get(1).unwrap().as_str());
        //     if let Err(why) = msg.channel_id.say(&ctx.http, "Pong!").await {
        //         println!("Error sending message: {:?}", why);
        //     }
        // }
    }

    // Set a handler to be called on the `ready` event. This is called when a
    // shard is booted, and a READY payload is sent by Discord. This payload
    // contains data like the current user's guild Ids, current user data,
    // private channels, and more.
    //
    // In this case, just print what the current user's username is.
    async fn ready(&self, ctx: Context, ready: Ready) {
        // let chan = ctx.http.get_channel(704582327384145962).await.unwrap();
        // chan.id().messages(http, builder)
        let channel_id = ChannelId(702062981608898614);
        // let last_msg = MessageId(812209739417124884);

        let messages = channel_id
            .messages(&ctx.http, |ret| ret.limit(1))
            .await
            .unwrap();
        for msg in messages {
            println!("{}", msg.content)
        }
        println!("{} is connected!", ready.user.name);
    }
}
async fn spotify_action() {
    // The default credentials from the `.env` file will be used by default.
    let mut oauth = SpotifyOAuth::default()
        .scope("user-follow-read user-follow-modify")
        .build();

    // In the first session of the application we only authenticate and obtain
    // the refresh token.
    println!(">>> Session one, obtaining refresh token:");
    let refresh_token = get_refresh_token(&mut oauth).await;

    // At a different time, the refresh token can be used to refresh an access
    // token directly and run requests:
    println!(">>> Session two, running some requests:");
    let spotify = client_from_refresh_token(&mut oauth, &refresh_token).await;
    print_followed_artists(spotify).await;

    // This process can now be repeated multiple times by using only the
    // refresh token that was obtained at the beginning.
    println!(">>> Session three, running some requests:");
    let spotify = client_from_refresh_token(&mut oauth, &refresh_token).await;
    print_followed_artists(spotify).await;
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let mut oauth = SpotifyOAuth::default()
        .scope("playlist-modify-public playlist-modify-private playlist-read-private")
        .build();
    // let mut refresh_token = String::new();
    let refresh_token = match env::var("SPOTIFY_REFRESH_TOKEN") {
        Ok(token) => token,
        Err(_) => get_refresh_token(&mut oauth).await,
    };

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    println!("{}", refresh_token);
    let mut client = Client::builder(&token)
        .event_handler(Handler {
            spotify_refresh_token: refresh_token,
            spotify_oauth: oauth,
        })
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}

async fn get_refresh_token(oauth: &mut SpotifyOAuth) -> String {
    let token = get_token_without_cache(oauth)
        .await
        .expect("couldn't authenticate successfully");
    token
        .refresh_token
        .expect("couldn't obtain a refresh token")
}

async fn client_from_refresh_token(oauth: &SpotifyOAuth, refresh_token: &str) -> Spotify {
    let token_info = oauth
        .refresh_access_token_without_cache(refresh_token)
        .await
        .expect("couldn't refresh access token with the refresh token");

    // Building the client credentials, now with the access token.
    let client_credential = SpotifyClientCredentials::default()
        .token_info(token_info)
        .build();

    // Initializing the Spotify client finally.
    Spotify::default()
        .client_credentials_manager(client_credential)
        .build()
}

// Sample request that will follow some artists, print the user's
// followed artists, and then unfollow the artists.
async fn print_followed_artists(spotify: Spotify) {
    let followed = spotify
        .current_user_followed_artists(None, None)
        .await
        .expect("couldn't get user followed artists");
    // println!(
    //     "User currently follows at least {} artists.",
    //     followed.artists.items.len()
    // );
    for artist in followed.artists.items.iter() {
        println!("following {}", artist.name)
    }
}

impl Handler {
    async fn add_song_to_playlist(&self, song: &str) {
        let spotify =
            client_from_refresh_token(&self.spotify_oauth, &self.spotify_refresh_token).await;
        let user = spotify.current_user().await.unwrap();
        spotify
            .user_playlist_add_tracks(
                &user.id,
                "6Bq10yvtBE5lCZi1Fgx6ol",
                &[song.to_string()],
                None,
            )
            .await
            .unwrap();
    }
    async fn message_handler(&self, msg: Message) {
        let re = Regex::new(r".*open.spotify.com/track/([^\s?]*)?.*").unwrap();
        if let Some(captures) = re.captures(msg.content.as_str()) {
            match captures.get(1) {
                Some(link_msg) => self.add_song_to_playlist(link_msg.as_str()).await,
                None => return,
            }
        }
    }
}
