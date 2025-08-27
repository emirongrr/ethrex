window.BENCHMARK_DATA = {
  "lastUpdate": 1756326406960,
  "repoUrl": "https://github.com/emirongrr/ethrex",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "72628438+avilagaston9@users.noreply.github.com",
            "name": "Avila Gastón",
            "username": "avilagaston9"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e019531dfdc25fb92e0413213d72eb9e7c2171f4",
          "message": "feat(l2): deposit rich accounts by default (#4184)\n\n**Motivation**\n\nCurrently, running `ethrex l2 --dev` doesn't make deposits because\n`../../fixtures/keys/private_keys_l1.txt` can't be found at runtime.\n\n**Description**\n\nEmbeds the contents of `private_keys_l1.txt` into the binary and uses\nthem when the `deposit_rich` flag is set without a private key path\nspecified.\n\nCloses None",
          "timestamp": "2025-08-27T19:25:22Z",
          "tree_id": "68622f52167242d73e4dd16b5dcd4e6c91872e41",
          "url": "https://github.com/emirongrr/ethrex/commit/e019531dfdc25fb92e0413213d72eb9e7c2171f4"
        },
        "date": 1756326405126,
        "tool": "cargo",
        "benches": [
          {
            "name": "Block import/Block import ERC20 transfers",
            "value": 158292890627,
            "range": "± 475209743",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}