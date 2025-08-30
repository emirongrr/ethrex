window.BENCHMARK_DATA = {
  "lastUpdate": 1756556377078,
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
      },
      {
        "commit": {
          "author": {
            "email": "me+git@droak.sh",
            "name": "Oak",
            "username": "d-roak"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "b5b710b098e709b9782d05df2811ad450acccd2c",
          "message": "fix(l1): out of bounds on jwt file (#4148)\n\n**Motivation**\n\nThere's a problem when `contents` value has fewer than 2 characters, it\npanics the program.\n\n**Description**\n\nThe program shouldn't panic with out-of-bounds errors. The solution was\nto replace it with the native check `starts_with()`",
          "timestamp": "2025-08-29T21:25:10Z",
          "tree_id": "db714cd50d3c5879a549ed1ff52f7532e6f8beed",
          "url": "https://github.com/emirongrr/ethrex/commit/b5b710b098e709b9782d05df2811ad450acccd2c"
        },
        "date": 1756556375537,
        "tool": "cargo",
        "benches": [
          {
            "name": "Block import/Block import ERC20 transfers",
            "value": 161189814241,
            "range": "± 221890562",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}