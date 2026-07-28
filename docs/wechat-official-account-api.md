# WeChat Official Account Draft API

MyOpenPanels saves WeChat Official Account articles through the documented
server-side draft API. It does not automate `mp.weixin.qq.com`, publish the
article, schedule it, or mass-send it.

## Direct API flow

Clicking **Publish** in the built-in WeChat row submits directly from Studio.
It does not create an Agent Task, load a Publishing Skill, or require a Task
Handoff. The server performs these operations once:

1. Obtain an Official Account `access_token` with the client-credential flow.
2. Upload the first ordered image as permanent cover material.
3. Upload each remaining image through the article-image endpoint and append
   the returned WeChat-hosted URLs after the escaped text body.
4. Submit one `POST /cgi-bin/draft/add` request.
5. Treat the returned draft `media_id` as the authoritative save confirmation.

After a successful submission, Publishing history shows a **View** link beside
the success icon. It opens the WeChat Official Account draft list. The API does
not return a stable dashboard detail URL for the new draft, so MyOpenPanels does
not fabricate one from the `media_id`.

The title is limited to 32 characters. Studio truncates longer titles and
article content before submission so the generated HTML stays within WeChat's
20,000-character and 1 MiB limits. Publishing tags are omitted because the
official draft API has no corresponding field. The stored submission result
lists every field that was truncated or omitted.

References:

- [Get access token](https://developers.weixin.qq.com/doc/offiaccount/Basic_Information/Get_access_token.html)
- [Add permanent assets](https://developers.weixin.qq.com/doc/offiaccount/Asset_Management/Adding_Permanent_Assets.html)
- [Add a draft](https://developers.weixin.qq.com/doc/subscription/api/draftbox/draftmanage/api_draft_add.html)

The draft API must be called from a server. Article images cannot use arbitrary
external URLs, and the cover must use a permanent media id.

## Configuration

The Publishing panel opens the WeChat API configuration dialog before direct
submission when no validated configuration exists. The dialog:

- detects and displays the Studio server's current public egress IP;
- links directly to the current AppID's WeChat Developer Platform Official
  Account page for the developer credentials and IP allowlist;
- validates the AppID and AppSecret with the token endpoint;
- validates the current IP allowlist and draft-management permission;
- shows the IP reported by WeChat when it differs from the independently
  detected Studio public IP;
- saves credentials only after every validation succeeds.

Saved credentials live outside Project content under the private MyOpenPanels
storage `secrets` directory. On Unix systems, the directory is mode `0700` and
the credential file is mode `0600`. The AppSecret is never returned by the
Studio API.

These environment variables remain supported as an initial configuration
source:

| Variable | Purpose |
| --- | --- |
| `MYOPENPANELS_WECHAT_APP_ID` | Official Account AppID |
| `MYOPENPANELS_WECHAT_APP_SECRET` | Official Account AppSecret |

Restart Studio after changing either environment value, then validate it once
through the configuration dialog. Do not put either value in a Task, Project
file, command argument, prompt, log, or `execution-result.json`. MyOpenPanels
never returns the AppSecret, access token, or credentials to the Agent.

The Studio host's public egress IP must be present in the Official Account API
IP allowlist, and the account must expose the draft-management API permission.
If the public IP changes, the next WeChat release opens the dialog and requires
validation again. Credential, allowlist, and permission errors returned by the
direct API call also reopen the dialog before a new submission is attempted.

## Submission behavior

Studio snapshots the selected Typesetting publication and records the direct
API result in Publishing history without creating a Task. A request id makes a
replayed browser request idempotent, and a started request without a recorded
response remains `unknown` rather than being retried automatically.

Expected outcomes:

| Outcome | Meaning |
| --- | --- |
| `published` | WeChat returned a new draft `media_id`; the article is saved in the draft box only |
| `needs_user_action` | Credentials, IP allowlist, or account API permission require configuration |
| `not_published` | Validation or a definitive WeChat API response rejected the request |
| `unknown` | The final `draft/add` request may have reached WeChat but no authoritative response was received |

An `unknown` result must never be retried automatically because a duplicate
draft could be created.

The official draft API has no topic field, so Publishing tags are recorded as
omitted instead of blocking the submission.
