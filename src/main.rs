use std::{collections::HashMap, env};

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
    channel_id: u64,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, _: Context, msg: Message) {
        // if this isn't the case we'll consume spotify links from ANY channel, which maybe isn't bad?
        if msg.channel_id.as_u64().eq(&self.channel_id) {
            self.message_handler(msg).await;
        }
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        let channel_id = ChannelId(self.channel_id);
        let mut stack: Vec<Message> = Vec::new();
        let mut msgs = MessagesIter::<Http>::stream(&ctx, channel_id).boxed();
        while let Some(msg) = msgs.next().await {
            match msg {
                Ok(m) => {
                    stack.push(m);
                }
                Err(err) => {
                    println!("{}", err);
                }
            }
        }
        // MessagesIter streams from newest to oldest, which doesn't really make sense for a playlist
        while let Some(msg) = stack.pop() {
            self.message_handler(msg).await;
        }

        println!("{} is connected!", ready.user.name);
    }
}

#[tokio::main]
async fn main() {
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let mut oauth = SpotifyOAuth::default()
        .scope("playlist-modify-public playlist-modify-private playlist-read-private")
        .build();

    let refresh_token = match env::var("SPOTIFY_REFRESH_TOKEN") {
        Ok(token) => token,
        Err(_) => get_refresh_token(&mut oauth).await,
    };

    let mut playlist_id =
        env::var("SPOTIFY_PLAYLIST").expect("expected SPOTIFY_PLAYLIST to be set");
    let channel_id = env::var("DISCORD_CHANNEL_ID")
        .expect("need channel id")
        .parse::<u64>()
        .expect("channel id not int");

    let mut song_map = HashMap::new();
    let spotify = client_from_refresh_token(&oauth, refresh_token.as_str()).await;
    // There's a lot of unwrap(), but this is just in the init code
    let user = spotify.current_user().await.unwrap();

    let playlist = spotify
        .user_playlist(
            user.id.as_str(),
            Some(playlist_id.as_mut_str()),
            Some("fields=tracks.items(id)"),
            None,
        )
        .await
        .unwrap();

    for track in playlist.tracks.items {
        let unwrapped_track = track.track.unwrap();

        match unwrapped_track.id {
            Some(track_id) => {
                println!("{}", track_id);
                song_map.insert(track_id, true);
            }
            _ => {}
        };
    }

    let mut client = Client::builder(&token)
        .event_handler(Handler {
            spotify_refresh_token: refresh_token,
            spotify_oauth: oauth,
            map: RwLock::new(song_map),
            spotify_playlist: playlist_id,
            channel_id,
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
        // This unwrap shouldn't happen, since it's just compiling the regex
        let re = Regex::new(r".*open.spotify.com/track/([^\s?]*)?.*").unwrap();
        if let Some(captures) = re.captures(msg.content.as_str()) {
            match captures.get(1) {
                Some(link_msg) => {
                    // if it's false, we haven't added the song to the playlist
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
