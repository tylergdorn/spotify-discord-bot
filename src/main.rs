use std::{collections::HashMap, env};

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
    // who knows why we need this
    spotify_oauth: SpotifyOAuth,
    map: HashMap<String, bool>,
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
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        let channel_id = ChannelId(702062981608898614);

        let messages = channel_id
            .messages(&ctx.http, |ret| ret.limit(1))
            .await
            .unwrap();
        let last_msg = &messages[0];
        loop {
            let messages = channel_id
                .messages(&ctx.http, |ret| ret.before(last_msg).limit(100))
                .await
                .unwrap();

            let msg_len = messages.len();
            for message in messages {
                self.message_handler(message).await;
            }
            if msg_len < 100 {
                break;
            }
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

    let mut song_map = HashMap::new();
    let spotify = client_from_refresh_token(&oauth, refresh_token.as_str()).await;
    let user = spotify.current_user().await.unwrap();
    let mut playlist_id = String::from("6Bq10yvtBE5lCZi1Fgx6ol");
    // let why = &mut *playlist_id;
    let playlist = spotify
        .user_playlist(
            user.id.as_str(),
            Some(playlist_id.as_mut_str()),
            Some("fields=tracks.items(id)"),
            None,
        )
        .await
        .unwrap();
    println!("{:#?}", playlist.tracks.next);

    for track in playlist.tracks.items {
        // println!("{:#?}", track);
        let unwrapped_track = track.track.unwrap();
        println!("{:#?}", unwrapped_track);

        match unwrapped_track.id {
            Some(track_id) => {
                // println!("{}", track_name);
                song_map.insert(track_id, true);
            }
            _ => {}
        };
    }
    println!("{:#?}", song_map);
    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    println!("{}", refresh_token);
    let mut client = Client::builder(&token)
        .event_handler(Handler {
            spotify_refresh_token: refresh_token,
            spotify_oauth: oauth,
            map: song_map,
        })
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    // client.
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
                Some(link_msg) => {
                    // if it's None, we haven't added the song to the playlist
                    if let None = self.map.get(link_msg.as_str()) {
                        self.add_song_to_playlist(link_msg.as_str()).await
                    }
                }
                None => return,
            }
        }
    }
}
