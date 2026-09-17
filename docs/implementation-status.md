# 구현 상태 — 2026-09-17

사용자 결정에 따라 **Go 기반을 Rust로 전환하고 macOS Apple Silicon을 첫 설치·운용 UX 대상으로 변경**했다. 현재 실제 기능은 development package 생성·검증과 CLI/제품 설정·로컬 metadata 검사다. installer·launchd·실제 엔진 연동은 다음 구현 범위다.

## 이번 변경과 실제 제공 범위

- Rust 1.86.0, Cargo.toml/Cargo.lock, fmt·Clippy·test·release build, Mac/Linux CLI CI 정의.
- `version`: Rust 구현·darwin-arm64 우선 target·capability·engine/service 미인수 상태.
- `package build`: expected JAR/Java hash·directory 기반 staging·결정적 tar/gzip·Ed25519·출력 충돌/변경 감지·자체 검증.
- `package verify`: 외부 신뢰 key·서명 원문·모든 파일 hash/size/mode·경로·형식·압축 한도·strict JSON. 파일 추출이나 JVM 실행 없음.
- Mac `darwin/arm64` development profile 추가, 기존 `linux/amd64/glibc` 유지. 잘못 섞은 OS/CPU/libc profile은 거부.
- `config validate`: 제품 strict JSON·경로·loopback HTTP·명시 P2P·파일 참조 검사.
- `preflight`: 파일/디렉터리 metadata만 검사. 모두 PASS여도 NIGO canonical·runtime·service 검사가 없으므로 INCOMPLETE/exit 5. 로컬 문제는 exit 4.
- JSON stdout 하나·secret 비노출·미구현 명령의 명시적 거부를 유지했다.

실제 engine/hash·runtime 공급물은 아직 없다. `channel=development`, `contractStatus=proposed`만 허용한다. Linux/Go 시절의 검증 기록은 [과거 foundation 기록](../results/2026-09-16-foundation.md)으로 보존하며 이번 결과는 [Rust 전환 검증](../results/2026-09-17-rust-migration.md)에 기록한다.

## 전체 계획 대비 상태

담당: BXDL 구현 세션. revision: 이 문서를 포함한 BXDL Git revision. 로컬 검증 시점은 연결된 results 기록을 따른다. NIGO 제공 owner·일정: 미합의. G1-M/G1-L/G1-D의 실제 engine/service 인수는 모두 미실행이다.

| 작업 | 상태 | 현재 결과와 잔여 조건 |
| --- | --- | --- |
| BX-001, BX-005 | IN_PROGRESS | 첫 macOS arm64 사용자 profile·test-owned 원칙·G1-M 계획 정의. 동봉 JRE/OS 최소 버전·launchd 실제 인수 대기 |
| BX-002 | DONE | Rust 기반·고정 toolchain/lock·Mac CLI 빌드·정적 검사·테스트. OS 서비스 지원 완료는 아님 |
| BX-003 | PLAN | 동봉 Java 21 공급자·patch·hash·NOTICE/SBOM 미선정 |
| BX-004 | IN_PROGRESS | NIGO REQ-0002에 Rust/Mac 우선·Linux 후속 제안 반영. OPEN 유지, 공급자 합의/commit 공유 대기 |
| BX-010 | DONE | Rust CLI routing·JSON/사람용 결과·exit·CLI identity·부정 사례 이식 |
| BX-011~014 | IN_PROGRESS | development build/verify·서명·manifest·Mac profile 구현. 공식 lock·공급 인증·extract/설치·정식 배포 미구현 |
| BX-020 | IN_PROGRESS | Rust 제품 JSON schema/validation 구현. 실제 NIGO canonical mapping 대기 |
| BX-021~022, BX-024 | BLOCKED | engine rendering·명시 init·실제 DTO는 NIGO-01~04 공급 필요. 다음 책임은 NIGO 제공 합의와 BXDL adapter 구현 |
| BX-023 | IN_PROGRESS | Rust 로컬 metadata 구현. engine cold·effective service access NOT_CHECKED |
| BX-030~034 | PLAN | Mac installer·journal·launchd·상태/진단/제거 먼저. Linux adapter는 후속 |
| BX-040~043 | PLAN | G1-M Mac exact package/UX 인수. G1-L Linux multi-host·G1-D Docker는 별도 후속 |
| BX-044 | IN_PROGRESS | Mac/Ubuntu Rust CLI workflow 정의. 원격 실행 결과는 해당 revision의 GitHub Actions에서 확인. engine/service 인수·release 미실행 |
| BX-050~053, M6 | PLAN | 버전 쌍 업데이트·offline 도구·고객 운영 확대는 후속 |

## 다음 구현 순서

[Mac 우선 설계](../design/2026-09-17-rust-macos-first.md)의 R1~R4에 따라 setup 입력·검사·오류 수정 UX → 검증된 Mac package/installer·instance journal → NIGO init/cold/start/stop와 launchd → 실제 로컬 콘솔·동일 데이터 재시작을 연결한다. setup 및 운영 명령은 아직 제공하지 않는다.

Mac의 사용자 설치는 CLI와 engine가 같은 UID라는 경계를 명시한다. Linux의 root 제어 기록·전용 서비스 사용자 격리와 같다고 주장하지 않는다. 로그인/로그아웃·잠자기·터미널 종료·부분 실패·재개를 UX 인수에 포함한다. Linux는 systemd·native·권한·다중 host를 별도 인수한다.

Rust 전환만으로 NIGO 계약 부재가 해소되지 않는다. NIGO-01~04/06은 초기 연결에 필요하며 05(offline tool)는 후속이다. 기존 실행 중인 node/data/키는 검증 fixture로 사용하지 않는다. BXDL 소스 공유와 공식 제품 릴리스는 구분한다. 공식 릴리스는 미발행이며 NIGO 문서 변경은 별도 세션에서 공유한다.
