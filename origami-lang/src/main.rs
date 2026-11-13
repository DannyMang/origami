fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: origami-lang <script.js>"))?;
    println!("{}", serde_json::to_string_pretty(&origami_lang::eval(path.as_ref(), true)?)?);
    Ok(())
}
