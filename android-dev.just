set shell := ["sh", "-eu", "-c"]

# Run one Android Emulator image-compatibility spike stage.
spike stage:
    sh scripts/android-spike-sdk.sh "{{ stage }}"
