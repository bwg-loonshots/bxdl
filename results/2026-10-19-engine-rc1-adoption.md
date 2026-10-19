# NIGO RC1 수신·등록 정책 안내·서명 Mac 패키지 인수

2026-10-19 · BXDL 기반 `70f527b` 이후 `feat/engine-rc1-adoption` 미커밋 변경.
[구현 설계](../design/2026-10-19-engine-rc1-adoption.md) · [수신 snapshot](../contracts/nigo/engine-rc1-2026-10-19/README.md) · [정제 결과 JSON](./2026-10-19-engine-rc1-adoption.json)

## 변경

NIGO REQ-0003의 공급 정책과 RC1 범위를 수용하고, private prerelease를 새 경로에 다운로드했다.
태그 full commit·JAR/manifest/archive·계약·console·inventory를 대조하고 원문 snapshot과 새 lock/spec 예시를 추가했다.
기존 303e snapshot과 예시는 보존한다. runtime 문서·정의·health fixture는 동일하며 기존 parser 판정을 완화하지 않았다.

설치 계획에 RC1의 OPEN/ALLOWLISTED·빈 목록 의미와 genesis 고정 안내를 추가했다. pin된 native/chain을 다시 읽고,
누락/잘못된 값·중복·OPEN+nonempty를 성공 자격으로 표시하지 않는다. 원문 주소·오류는 출력하지 않는다.
새 정보는 기존 Prepared/Plan/journal의 직렬화·hash·equality에 넣지 않아 303e 계획의 재개 형식을 유지한다.

## 자동 검증

| 검사 | 결과 |
| --- | --- |
| `make check` | fmt, all-targets Clippy `-D warnings`, 280 PASS / 0 failed / 0 ignored |
| Mac `cargo build --locked --release` | PASS, Rust 1.86.0, arm64 |
| Linux `cargo check --locked --target x86_64-unknown-linux-gnu` | PASS; Linux 실행 인수 아님 |
| 별도 코드 리뷰 | setup pin/입력 표시·기존 journal 호환·주소 미출력 경계 검토, 발견 결함 없음 |

280개는 library244, CLI13, CLI integration2, setup CLI5, setup install11, setup safety5다.
RC1 engine-info를 사용한 init/resume/run/bootstrap 기록은 합성 회귀이며 실제 실행 근거와 구분한다.
공급 health15개 golden의 상태·큰 decimal·알 수 없는 reason 거절과 diagnostics 정제를 확인한다.

## 실제 사용한 조합

| 항목 | 값 |
| --- | --- |
| NIGO source | `540a89f0814c01d2a4ec67da1087af35195882ca`, dirty=false |
| JAR SHA-256 | `f1ea80e25052221ed8a6e3108e020cb1b776833f4f86a29aae0885ad41703f45` |
| CLI SHA-256 / bytes | `c5d62bc3ddbd2e61557a4f9954dfde67894d104e049f6f05f121256bc4688fac` / 2,625,312 |
| BXDL 구현 source SHA-256 | `7ecb3ef57167d23119acdb051ee1ee532269f26a31e9f6caaf3c5a128f649b06` |
| 제품 archive SHA-256 | `421dd69ad14454b2820b2f4e02e2739e035eb98fe2220c5a9f6718e6cc458300` |
| 제품 manifest SHA-256 | `ffd4790ff5758ec8a873e853579020d4b02823c429b5c40e024ea1adc7a7e5d0` |
| 서명 / payload | 외부 Ed25519 시험 key, `verified-external-ed25519`, 141파일 / 253,437,334 bytes |
| 시험 OS | macOS26.6.2 (25G83), arm64 |
| 시험 JRE | 기존 test-only Oracle21.0.7 jlink, java.se·jdk.crypto.ec·jdk.unsupported·jdk.management 포함 |
| Java launcher SHA-256 | `94b66f2cc8edfba9e0e8b25abe93e4c3617d2cc1af7e8eaec0f5b2afe0df5c12` |

구현 digest는 `Cargo.toml`, `Cargo.lock`, `src/**/*.rs`를 상대 POSIX 경로 문자열순으로 정렬하고 relative POSIX path + NUL + bytes + NUL을
누적한 SHA-256이다. 제품 revision은 지원하는 `development` 값이며 미커밋 구현의 추적은 위 digest와 CLI hash를 따른다.
선정한 실제 runtime 전체 파일은 package inventory로 검증했다. 고객 재배포 JRE 선정·완전한 NOTICE/SBOM 또는 Apple 서명이 아니다.

## 실제 시험 결과

원격 수신 JAR을 재빌드하지 않고 동일 release CLI와 함께 서명했다. 새 경로에 기존 시험 NGVK를 복사하고,
만료된 이전 TLS 인증서를 사용하지 않도록 새 test-only PKCS12 key/truststore·password와 30일 인증서를 생성했다.
시험 시작 시 공개 인증서의 유효기간을 확인했다. 기존 DB·설정·PKI는 변경하지 않았다.

| 검사 | 결과 |
| --- | --- |
| package build/verify | 외부 key 서명·141파일 inventory PASS |
| 실제 JRE의 RC1 engine inspect | 전체 identity MATCHED |
| ALLOWLISTED+empty / OPEN+empty cold | 둘 다 INCOMPLETE/exit5; runtime 미검사 보존 |
| 모드 누락 / OPEN+nonempty / 중복 issuer cold | 모두 ENGINE_INVALID_CONFIGURATION/exit3, data 미생성 |
| 실제 TTY archive 뒤 취소→재개 | exit5, checkpoint 보존, 설치/control/data 미생성 |
| `--from` 없는 전체 14문답 | 입력·모드 ALLOWLISTED 안내·계획 확인 PASS |
| 서명 package apply/등록 | 설치된 CLI 일치, cold MATCHED/INCOMPLETE, 초기화 전 data 없음 |
| 별도 init 확인 | INITIALIZED, 실제 engine-instance genesis 대조 |
| 별도 start 확인 | LaunchAgent/engine RUNNING, 로컬 READY, 전역 합의 NOT_CHECKED |
| 정상 stop | SERVICE_STOPPED_VERIFIED, job NOT_LOADED·engine STOPPED |
| 동일 data 재시작 | 새 attempt/nodeInstanceId, 같은 genesis, RUNNING/READY |
| 두 번째 stop 및 setup 재개/finish | 정상 종료·추가 engine attempt 없음, busy=false |

최종 서비스는 NOT_LOADED, 엔진은 STOPPED다. consensus auto-start와 sync OFF, peer 없는 단일 validator fixture이므로
4-validator 거래 확정·합의 수렴을 검증한 결과가 아니다.

최초 시험 준비에서는 harness가 지원하지 않는 product revision 문자열을 써 package build가 MANIFEST_INVALID로 거절했다.
노드/DB는 시작하지 않았고 실패 자료를 보존했다. harness만 `development`로 고쳐 새 경로에서 위 전체 흐름을 통과했다.
생산 코드·gate·timeout을 바꾸거나 실패 시도를 PASS로 덮어쓰지 않았다.

로컬 최종 자료는 `/private/tmp/bxdl-rc1-setup-bzyk_dp0`, 준비 실패 자료는 `/private/tmp/bxdl-rc1-setup-8of1jrzf`다.
원격 수신 검증은 `/private/tmp/bxdl-rc1-receipt-iz3vx_dt`, 실행 harness는 작업 폴더
`work/bxdl-rc1-acceptance/smoke_setup_rc1.py`에 보존했다. 경로들은 개발 증거 위치이며 고객 실행 의존성이 아니다.
Git에는 실행파일·archive·키·전체 설정·DB·원시 PTY/로그를 넣지 않았다.

## 남은 작업

- BXDL lock/snapshot 변경과 NIGO 소비자 회신의 commit/PR 공유.
- BXDL source-free QBFT 설정·PKI·거래 fixture 준비 및 동일 package의 4-validator 합의/재시작 인수.
- setup 실제 강제 중단·실패 주입, 로그인/로그아웃·sleep/wake·격리 오프라인과 전체 G1-M.
- 정식 JRE/최소 OS/라이선스·NOTICE/SBOM, Linux/systemd·Docker, DB upgrade/downgrade와 운영 출시.

REQ-0003 공급·수신·단일 노드 lifecycle의 지정 범위를 확인했으며 전체 요청 VERIFIED나 정식 출시로 승격하지 않는다.
