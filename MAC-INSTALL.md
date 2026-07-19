# Installing Murmr on macOS

Murmr is code-signed and notarized by Apple, so it installs like any normal
Mac app — no Terminal commands, no Gatekeeper workarounds. The only thing
that takes a moment is granting the three macOS permissions Murmr needs to
do its job. Once installed, future updates install themselves automatically
through Murmr's built-in updater.

This whole walkthrough takes about 60 seconds.

---

## 1. Download the right file

Open **https://github.com/composrr/Murmr/releases/latest** and download:

- **Apple Silicon (M1 / M2 / M3 / M4)** → `Murmr_<version>_aarch64.dmg`
- **Intel Mac** → `Murmr_<version>_x64.dmg`

Not sure which Mac you have? → Apple menu → **About This Mac** → look at "Chip"
or "Processor". If it says **Apple M-something**, you want aarch64. Anything
with **Intel** in the name → x64.

---

## 2. Install

Open the downloaded `.dmg` and drag **Murmr** into your **Applications**
folder. Then double-click Murmr in Applications — it opens straight away.
Because the app is notarized, Gatekeeper recognizes it and won't show the
"damaged" or "unidentified developer" warnings that unsigned apps trigger.

---

## 3. Grant three permissions on first launch

Murmr needs three macOS permissions to work. The system prompts you for them
the first time each is needed — but on Mac, **two of them require restarting
Murmr after you grant them** (a quirk of the OS, not Murmr).

Here's the cleanest order:

### a) Input Monitoring — prompted at launch

The very first time Murmr starts, macOS will show:

> **"Murmr would like to monitor input from your keyboard."**

Click **Allow**. This is what lets Murmr's global hotkey work — without it,
pressing Right Ctrl (or whatever you've set) does nothing.

**You'll need to quit and reopen Murmr after granting this** — macOS only
applies new Input Monitoring permissions on app restart. Murmr will pop a
banner reminding you.

> If you missed the prompt, open
> **System Settings → Privacy & Security → Input Monitoring**, find Murmr in
> the list, and toggle it on.

### b) Microphone — prompted on first recording

The first time you press your dictation hotkey to record, macOS asks:

> **"Murmr would like to access the microphone."**

Click **OK**. Audio capture works immediately, no restart needed.

> If you click "Don't Allow" by accident, fix it at
> **System Settings → Privacy & Security → Microphone**.

### c) Accessibility — prompted on first paste

The first time Murmr tries to paste a transcription into another app, macOS
asks:

> **"Murmr requires accessibility access to control your computer."**

Click **Open System Settings**, find Murmr in the list under
**Accessibility**, and toggle it on. Without this, Murmr can transcribe
audio fine but can't actually type the result into your focused field.

This **also requires quitting and reopening Murmr** for the toggle to take
effect.

---

## 4. (Optional) Confirm everything works

After both restarts, run through Murmr's onboarding wizard once. It walks
you through a hotkey check, a mic test, and a practice transcription.
If any step fails, the most likely cause is one of the three permissions
above wasn't granted (or wasn't applied because Murmr wasn't restarted).

To re-check permissions later:
- **Microphone**: System Settings → Privacy & Security → Microphone
- **Accessibility**: System Settings → Privacy & Security → Accessibility
- **Input Monitoring**: System Settings → Privacy & Security → Input Monitoring

Each pane has Murmr listed with a toggle.

---

## Updating

Once installed, Murmr updates itself. Click the version number in
**Settings → General → "Check for updates"**, or wait for the periodic
auto-check. Updates download in the background and apply on the next quit.
Because updates are signed with the same Apple Developer ID, your granted
permissions carry over — you won't have to repeat any of this.

---

## Uninstalling

Drag `Murmr.app` from Applications to the Trash. To also remove its data
(transcription history, settings, dictionary, sounds), delete:

```
~/Library/Application Support/app.murmr.desktop/
```

---

## Stuck?

If something above didn't work the way it's described, open an issue at
https://github.com/composrr/Murmr/issues and include:

- Your Mac model + macOS version (Apple menu → About This Mac).
- Which step you got stuck at.
- Any error message text.

I'll help you sort it out.
