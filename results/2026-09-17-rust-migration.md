# Rust 전환 검증 — 2026-09-17

## 대상과 주장 범위

BXDL 최초 커밋 전 로컬 작업 트리의 development CLI `0.1.0-dev`, revision `development`를 검증했다. 실제 NIGO/JRE 공급물은 없으며 package 테스트는 fake engine/runtime bytes와 일회용 test signing key를 사용했다. macOS installer·launchd·NIGO 실행을 검증한 결과가 아니다.

환경은 Darwin arm64, rustc 1.86.0 (05f9846f8 2025-03-31), cargo 1.86.0 (adf9b6ad1 2025-02-28)이다. Cargo.lock과 지정 의존성을 사용하며 dependency fetch 후 최종 검사·빌드는 작업 전용 cache/target에서 offline으로 수행했다.

## 실행 결과

| 실행 | 결과 |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets -- -D warnings` | PASS |
| `cargo test --locked` | 41 tests PASS, 0 failed/ignored |
| `make build` | Rust release host CLI 빌드 PASS |
| `make build-macos` | aarch64-apple-darwin release 빌드 PASS |
| `cargo check --locked --target x86_64-unknown-linux-gnu` | PASS. type-check이며 Linux linking/실행은 아님 |
| native CLI smoke 4개 | version·config 성공, 예시 참조 누락 preflight·미구현 start의 예상 실패 PASS |
| Go↔Rust package 교차검증 10 CLI 호출 | 기존 Linux v1 서명 package 양방향 검증 및 변조 거부 PASS |

Tests는 artifact verifier 12개, builder 9개, config 11개, CLI 7개, CLI 통합 2개다. CLI 통합은 실제 `Run` 경계로 signed Mac build→trusted verify→wrong key/변조/기존 output 거부와 config→INCOMPLETE/exit 5→권한 실패/exit 4를 연결했다. non-UTF8 process 인자 거부는 빌드된 테스트 CLI subprocess로 확인했다.

Mac smoke의 네 결과 모두 stdout 단일 JSON·빈 stderr를 확인했다.

| 명령 | exit / reasonCode |
| --- | --- |
| `bxdl version --json` | 0 / VERSION; implementation=rust, primaryTarget=darwin-arm64 |
| `bxdl config validate --file config/examples/instance.development.json --json` | 0 / PRODUCT_CONFIG_VALIDATED |
| `bxdl preflight --config config/examples/instance.development.json --json` | 4 / LOCAL_CHECK_FAILED; 실제 참조 자료 없는 예시의 예상 실패 |
| `bxdl start --instance demo --json` | 4 / CAPABILITY_NOT_IMPLEMENTED |

## 이전 구현과의 호환 확인

이전 Go binary SHA-256은 `351e83965ed37ea413dfc11651688ddf16d1a65af0794dd0be5f34ce14ef4066`이다. repo에서 제거하기 전에 이 binary와 이번 Rust release로 임시 fixture를 교차 소비했다.

1. Go build의 signed Linux archive를 Rust verify가 검증하고 manifest·archive/manifest hash·파일/byte count·authenticity가 동일함을 확인했다.
2. Rust build의 signed Linux archive를 Go verify가 같은 결과로 검증했다.
3. 같은 Rust 입력으로 두 번 조립한 archive bytes가 동일했다. Go와 Rust의 compressor 출력 bytes 동일성은 요구하지 않는다.
4. Go archive의 서명 manifest를 유지하고 payload 한 바이트를 바꾼 뒤 gzip CRC를 다시 만들었을 때 두 구현 모두 HASH_MISMATCH/exit 3으로 거부했다.
5. 새 darwin/arm64 signed archive는 Rust에서 통과하고 과거 Go에서는 PLATFORM_UNSUPPORTED/exit 3으로 거부했다. 신규 platform profile의 예상 차이다.

Go 소스·test·go.mod·.go-version·Go CI/build 경로는 Rust로 대체했다. 이전 Go 테스트 결과는 날짜별 역사로만 보존한다. Go compiler는 현재 BXDL 빌드/테스트 의존성이 아니다.

## 회귀 범위와 리뷰

USTAR physical header/extension·traversal·link·중복/누락 inventory·size/hash/mode·크기/개수/압축 한도·gzip CRC/trailing member·PEM/외부 trust·정확한 manifest bytes 서명을 검증했다. Builder는 expected hash, 열린 directory 기준 staging, signing key hardlink, snapshot 이후 내용/경로 변화, output 충돌과 자기 파일만 정리를 검사했다.

Config는 JSON duplicate/unknown/case alias/null/필수·타입·한도, 상대경로·공백·secret canary·읽기 전용 metadata·권한·symlink·P2P broadcast를 검사했다. 모두 로컬 metadata PASS여도 엔진 검사가 없어 INCOMPLETE를 유지했다.

독립 교차 검토에서 Go Unicode lower-case 정책과 Rust ASCII-only lower-case 차이를 발견해 수정했고 `docs/Keys/a`, `docs/a.Key` 거부 회귀를 추가했다. JSON 구조 오류/타입 오류의 v1 reasonCode 구분도 복원했다. Rust verifier는 추가 USTAR reserved/numeric/string 필드를 엄격하게 검사한다. Go가 만들고 검증한 정상 v1 archive 교차검증은 통과했다.

## 생성한 실행파일

| 파일 | SHA-256 |
| --- | --- |
| `bin/bxdl` | `e66ab6dd7b24459028a6db09b58e2d4421c64368ace352b67724fb69844147b5` |
| `dist/bxdl-darwin-arm64` | `f3c4d3219ec30c2378eee3e191507d2deeb2a1a97b3b1a06545f2dd478ac8370` |

둘 다 `file`에서 Mach-O 64-bit arm64로 확인했다. 로컬 개발 결과이며 Git ignore 대상이다. Apple Developer ID/notarization·공식 제품 release로 발행하지 않았다. 기존 Go Linux binary는 현재 Rust 결과와 혼동하지 않도록 제거한다.

## 미실행과 남은 조건

Linux linking/실행·systemd, Mac installer/launchd/login/sleep, 실제 동봉 JRE/native·NIGO init/cold/start/stop·동일 DB/WAL 재시작·4-validator, Docker, 원격 CI, 공식 공급 인증·release 발행은 미실행이다. package signature 검증은 Apple code signing 또는 공식 NIGO source→binary 출처 검증의 대체가 아니다.

[최신 구현 상태](../docs/implementation-status.md)와 [Rust·Mac 우선 설계](../design/2026-09-17-rust-macos-first.md)를 따른다.
