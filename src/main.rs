use std::{cell::RefCell, collections::HashMap, env};

use serenity::{
    async_trait,
    futures::StreamExt,
    http::Http,
    model::{
        channel::{Message, MessagesIter},
        gateway::Ready,
        id::ChannelId,
    },
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
    spotify_oauth: SpotifyOAuth,
    map: RwLock<HashMap<String, bool>>,
    spotify_playlist: String,
}

#[async_trait]
impl EventHandler for Handler {
    // Set a handler for the `message` event - so that whenever a new message
    // is received - the closure (or function) passed will be called.
    //
    // Event handlers are dispatched through a threadpool, and so multiple
    // events can be dispatched simultaneously.
    async fn message(&self, _: Context, msg: Message) {
        // println!("got a message");
        self.message_handler(msg).await;
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        let channel_id = ChannelId(702062981608898614);
        let mut msgs = MessagesIter::<Http>::stream(&ctx, channel_id).boxed();
        while let Some(msg) = msgs.next().await {
            match msg {
                Ok(m) => {
                    self.message_handler(m).await;
                }
                Err(err) => {
                    println!("{}", err);
                }
            }
        }

        // This attempts to insert all messages into the playlist
        // let messages = channel_id
        //     .messages(&ctx.http, |ret| ret.limit(1))
        //     .await
        //     .unwrap();
        // let mut last_msg = &messages[0];
        // loop {
        //     let messages = channel_id
        //         .messages(&ctx.http, |ret| ret.before(last_msg).limit(2))
        //         .await
        //         .unwrap();

        //     let msg_len = messages.len();
        //     for message in messages {
        //         println!("{}", message.content);
        //         self.message_handler(message).await;
        //         last_msg = &message.clone();
        //     }
        //     if msg_len < 2 {
        //         break;
        //     }
        // }

        println!("{} is connected!", ready.user.name);
    }
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
    println!("{}", refresh_token);
    let mut playlist_id =
        env::var("SPOTIFY_PLAYLIST").expect("expected SPOTIFY_PLAYLIST to be set");

    let mut song_map = HashMap::new();
    let spotify = client_from_refresh_token(&oauth, refresh_token.as_str()).await;
    let user = spotify.current_user().await.unwrap();
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
    println!("PLAYLIST {:#?}", playlist);

    for track in playlist.tracks.items {
        println!("{:#?}", track);
        let unwrapped_track = track.track.unwrap();
        println!("unwrapped: {:#?}", unwrapped_track);

        match unwrapped_track.id {
            Some(track_id) => {
                println!("{}", track_id);
                song_map.insert(track_id, true);
            }
            _ => {}
        };
    }
    println!("SONG MAP");
    println!("{:#?}", song_map);
    println!("FBAT ZNC");

    let mut client = Client::builder(&token)
        .event_handler(Handler {
            spotify_refresh_token: refresh_token,
            spotify_oauth: oauth,
            map: RwLock::new(song_map),
            spotify_playlist: playlist_id,
        })
        .await
        .expect("Err creating client");

    println!("Spotify playlist initialized");
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

impl Handler {
    async fn add_song_to_playlist(&self, song: &str) {
        let spotify =
            client_from_refresh_token(&self.spotify_oauth, &self.spotify_refresh_token).await;
        let user = spotify.current_user().await.unwrap();
        spotify
            .user_playlist_add_tracks(&user.id, &self.spotify_playlist, &[song.to_string()], None)
            .await
            .unwrap();
    }
    async fn message_handler(&self, msg: Message) {
        let re = Regex::new(r".*open.spotify.com/track/([^\s?]*)?.*").unwrap();
        if let Some(captures) = re.captures(msg.content.as_str()) {
            match captures.get(1) {
                Some(link_msg) => {
                    // if it's None, we haven't added the song to the playlist
                    println!("{}", link_msg.as_str());
                    let in_map = match self.map.read().await.get(link_msg.as_str()) {
                        Some(_) => true,
                        None => false,
                    };

                    if !in_map {
                        self.map
                            .write()
                            .await
                            .insert(link_msg.as_str().to_string(), true);
                        self.add_song_to_playlist(link_msg.as_str()).await;
                    }
                }
                None => return,
            }
        }
    }
}
