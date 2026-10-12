# LocalVQE

Echo removal for meetings: takes the call's sound out of the microphone when
there are no earphones.

- `localvqe-v1.4-aec-200K-f32.gguf`: the v1.4 echo-only model from
  [LocalAI-io/LocalVQE](https://huggingface.co/LocalAI-io/LocalVQE) (revision
  `29ca38495cba9d6393a92a4dd890f28dd81f758d`, SHA-256
  `b6e43138588a83bfe903ab5e143b4020b91c1e1629f5a575ac5855ff0003c731`).
- `liblocalvqe.dylib`: built from
  [localai-org/LocalVQE](https://github.com/localai-org/LocalVQE) by
  `scripts/build_localvqe.sh`; not committed.

LocalVQE is © its authors, licensed under the Apache License 2.0.
