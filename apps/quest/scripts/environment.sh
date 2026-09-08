#!/usr/bin/env bash
# Sourced by development commands. Do not change the user's shell configuration.
quest_cache="${HOME}/Library/Caches/Norma/quest-toolchain"
if [[ -z "${JAVA_HOME:-}" && "$(uname -s)" == Darwin ]]; then
  if quest_java=$(/usr/libexec/java_home -v 17 2>/dev/null); then
    export JAVA_HOME="$quest_java"
  else
    for quest_java in "$quest_cache"/jdk-17*/Contents/Home; do
      if [[ -x "$quest_java/bin/java" ]]; then export JAVA_HOME="$quest_java"; break; fi
    done
  fi
fi
if [[ -z "${ANDROID_HOME:-}" ]]; then
  if [[ -n "${ANDROID_SDK_ROOT:-}" ]]; then
    export ANDROID_HOME="$ANDROID_SDK_ROOT"
  elif [[ -d "$HOME/Library/Android/sdk" ]]; then
    export ANDROID_HOME="$HOME/Library/Android/sdk"
  elif [[ -d "$quest_cache/android-sdk" ]]; then
    export ANDROID_HOME="$quest_cache/android-sdk"
  fi
fi
if [[ -n "${JAVA_HOME:-}" ]]; then export PATH="$JAVA_HOME/bin:$PATH"; fi
if [[ -n "${ANDROID_HOME:-}" ]]; then export PATH="$ANDROID_HOME/platform-tools:$PATH"; fi
