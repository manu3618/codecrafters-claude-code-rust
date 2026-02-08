use async_openai::{Client, config::OpenAIConfig};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs::read_to_string;
use std::{env, process};

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Tool {
    Read,
}

impl Tool {
    fn to_spec(&self) -> Value {
        match &self {
            Self::Read => json!({
                "type": "function",
                "function": {
                    "name": "Read",
                    "description": "Read and return the contents of a file",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "The path to the file to read"
                            }
                        },
                        "required": ["file_path"]
                    }
                }
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: FunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionCall {
    name: Tool,
    //arguments: HashMap<String, String>,
    arguments: String,
}

impl FunctionCall {
    /// Execute a tool call
    fn execute(&self) -> String {
        match &self.name {
            Tool::Read => self.read(),
        }
    }
    fn read(&self) -> String {
        let arguments: HashMap<String, String> = serde_json::from_str(&self.arguments).unwrap();
        dbg!(&arguments);
        let file_path = arguments.get("file_path").unwrap();
        read_to_string(file_path).unwrap()
    }
}

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'p', long)]
    prompt: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let base_url = env::var("OPENROUTER_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    let api_key = env::var("OPENROUTER_API_KEY").unwrap_or_else(|_| {
        eprintln!("OPENROUTER_API_KEY is not set");
        process::exit(1);
    });

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);

    let client = Client::with_config(config);

    #[allow(unused_variables)]
    let tools = Tool::Read;
    let response: Value = client
        .chat()
        .create_byot(json!({
            "messages": [
                {
                    "role": "user",
                    "content": args.prompt
                }
            ],
            "tools": [tools.to_spec()],
            "model": "anthropic/claude-haiku-4.5",
        }))
        .await?;

    // You can use print statements as follows for debugging, they'll be visible when running tests.
    eprintln!("Logs from your program will appear here!");

    // TODO: Uncomment the lines below to pass the first stage
    dbg!(&response);
    if let Some(tool_calls) = response["choices"][0]["message"]["tool_calls"].as_array() {
        let tool_call = tool_calls.first().unwrap();
        // TODO: remove the ugly transofomation JSON -> String -> JSON
        let tool_call = tool_call.to_string();
        eprintln!("AAAA\n{}", &tool_call);
        let tool_call: ToolCall = serde_json::from_str(&tool_call).unwrap();
        dbg!(&tool_call);
        let result = tool_call.function.execute();
        println!("{result}");
    }

    if let Some(content) = response["choices"][0]["message"]["content"].as_str() {
        println!("{}", content);
    }

    Ok(())
}
