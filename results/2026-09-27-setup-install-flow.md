# setup 설치 흐름 구현·검증

2026-09-27 · BXDL 기반 `977752b` 이후 개발 변경 · NIGO clean `303e163a` 계약 유지.

## 구현 결과

명시적인 `setup --install --workspace`와 `--resume`을 추가했다. 한국어 TTY에서 패키지/신뢰 자료와 제품·native 설정을 확인하고, 파일 설치·등록, 새 DB 초기화, LaunchAgent 시작을 각각 선택한다. 기존 `setup` 초안 모드·Draft v1·비대화형 export는 유지했다.

새 세션의 workflow/draft/generated 저장소와 전체 세션 잠금을 분리했다. 현재 CLI가 선택한 패키지의 CLI와 다르면 설치 전에 거부한다. 계획 이후 archive·lock·설정·참조 자료 변경, 경로 겹침과 기존 데이터 사용을 거부하며 원본 자료를 보존한다.

설치·등록 root를 새로 만든 순간의 device/inode를 영속 기록한 뒤 본문 작성을 진행한다. 해당 소유 증거와 전체 receipt/inventory/binding을 검증할 수 있는 완료 결과만 중단 후 재구성한다. partial·소유 증거 누락·엔진 UNKNOWN을 자동 재실행하거나 성공으로 채택하지 않는다.

경로 기본값은 workspace의 형제 package/control/data다. 기존 부모가 필요하며 Application Support 상위 폴더 자동 생성은 별도 후속이다. GC는 입력의 ON/OFF 표시, issuer 목록은 초기화 전 확인 안내만 제공한다. 최신 NIGO 후보의 pin이나 요구 원장은 변경하지 않았다.

## 자동 검증

| 검사 | 결과 |
| --- | --- |
| 인수 전 로컬 `make check` | fmt, all-targets Clippy `-D warnings`, 270개 테스트 통과 |
| `cargo build --locked --release` | macOS arm64 통과 |
| `cargo check --locked --target x86_64-unknown-linux-gnu` | 타입 검사 통과; Linux 실행 인수 아님 |
| `git diff --check` | 통과 |

실제 패키지 인수 전 테스트 집계는 library 234, CLI 13, CLI integration 2, setup CLI 5, setup install 11, setup safety 5다. doc test와 main 단위 테스트는 0건이다. 새 작업 트리의 전용 target에서 컴파일·실행했으며, 이전 checkout의 캐시나 이름이 일치하지 않는 0건 실행을 검증 근거로 사용하지 않았다.

setup install 통합시험은 실제 현재 CLI bytes와 외부 Ed25519 키로 서명한 시험 archive, 가짜 Java 프로세스를 사용한다. 실제 파일 설치·cold/등록 연결, 계획 중 취소/재개, init 거절, CLI 불일치·참조 변경·기존 data 거부, 가짜 init 실패 후 UNKNOWN 보존을 검증했다. 등록 완료 직후 workflow 완료 checkpoint가 없어진 경우의 소유 결과 재구성, 소유 증거 없는 동일 결과 비인수, 확인 프롬프트에서 취소 시 이후 명령 미소비도 포함한다. 이 시험은 실제 JVM·DB·LaunchAgent 실행 증거가 아니다.

처음 sandbox 안에서 기존 loopback TCP/Unix socket 테스트 2건이 `Operation not permitted`로 실패했다. 테스트 전용 로컬 소켓을 허용한 최종 `make check`에서는 누락·skip 없이 통과했다.

## 검증 실행 파일

- OS: macOS 26.6.2, arm64.
- Rust: 저장소 고정 1.86.0.
- CLI 크기: 2,608,784 bytes.
- CLI SHA-256: `ac5c67d1ba7013665f74d730db08f7d6af0ad49b2bc5c2c2897e150898223163`.
- 구현 source SHA-256: `a35724644e27271a76560d2aaca84a7def502b3c1157e116c9a42268a6ca9c4a`.

구현 digest는 `Cargo.toml`, `Cargo.lock`, `src/**/*.rs`를 Python `sorted(Path 목록)`으로 정렬하고 repository-relative POSIX 경로와 bytes를 `path + NUL + raw bytes + NUL`로 누적했다. 커밋 전 개발 빌드이며 이 digest를 서명된 공급 attestation으로 취급하지 않는다.

### PR CI 후속

첫 PR CI의 Mac 검사는 통과했으나 Ubuntu에서는 기존 managed cancellation 시험이 실행 전 준비 단계에서 `INSTANCE_BUSY`로 실패했다. 준비용 lock을 해제한 직후 재획득하는 구간에서 병렬 fork가 복제한 descriptor가 exec까지 잠시 남을 수 있다. fork/CLOEXEC/flock의 동일 메커니즘을 별도 재현했다.

`src/engine/managed_tests.rs`에서 최초 준비 lock을 그대로 runner에 넘기도록 수정했다. 취소 중 BUSY, TERM 전달, 자식 생존, 종료 뒤 잠금 해제 검증은 유지했다. 후속 Ubuntu 검사는 통과했으며 Mac의 setup 통합시험에서도 다른 시험의 fork와 세션 drop/resume 간 일시적인 잠금 간섭이 드러났다. `tests/setup_install.rs`의 독립 시험들을 직렬화하고 소유 세션이 살아 있을 때 즉시 BUSY를 반환하는 검사를 추가했다. drop 뒤 재개는 재시도 없는 단발 호출을 유지한다.

별도 Ubuntu 실행에서 기존 Store 동시 저장 시험의 성공 반환 개수 가정도 드러났다. `save`는 독점 게시 후 다시 상태를 검사하므로 오류 반환이 게시의 롤백을 뜻하지 않는다. 정확한 Ubuntu 타이밍은 Mac에서 재현하지 못했다. 시험을 보완해 최종 체크포인트 하나·정확한 bytes·성공 호출과의 대응·권한·inode·temp 정리·덮어쓰기 거부·후속 revision에서 원본 보존을 확인하고, 원자적 게시 자체의 정확히 한 성공과 한 충돌을 별도 시험으로 유지했다. 두 동시 시험은 Mac에서 200회씩 반복 통과했다.

테스트가 한 개 늘어 Mac 전체 구성은 271개(library 235와 나머지 36)다. 재시도·대기 한도 확대·생산 코드 변경은 없다. 위 source digest는 실제 패키지 인수 당시 snapshot이며 이후 변경은 테스트 fixture들과 문서뿐이다. 최종 원격 결과는 [PR #7 검사](https://github.com/bwg-loonshots/bxdl/pull/7/checks)를 따른다.

## 실제 패키지 인수

위 release CLI와 동일한 bytes를 `bin/bxdl`에 넣은 새 unsigned-development archive를 만들고, 실제 Mac PTY에서 `setup --install`을 실행했다. 기존 시험의 clean `303e163a` JAR·JRE·chain·시험용 키를 읽기 전용으로 복사했고 DB·control·operation은 새로 만들었다. 원본 DB/control과 고객 자료는 시험 대상으로 삼지 않았다.

| 검증 | 관측 결과 |
| --- | --- |
| archive 입력 뒤 `:cancel` → 같은 세션 `--resume` | exit 5와 checkpoint 유지. 취소 시 package/control/data 미생성 |
| `--from` 제품 설정, 공백을 포함한 새 경로, 명시 `apply` | 파일 설치·등록 성공, 설치 CLI hash 일치 |
| 등록 직후 cold 및 instance 확인 | `configurationBinding=MATCHED`, cold `INCOMPLETE`, `initialization=NOT_STARTED`, data 미생성 |
| 별도 동의 후 `init` | `INITIALIZED`, 실제 engine-instance의 genesis 대조 |
| 별도 동의 후 `start` | LaunchAgent와 엔진 `RUNNING`, 로컬 readiness `READY`, 전역 합의 `NOT_CHECKED` |
| 설치 CLI로 첫 `stop` | `SERVICE_STOPPED_VERIFIED`, job `NOT_LOADED`, engine `STOPPED`, busy=false |
| 같은 설치 CLI·DB로 재시작 | 새 attemptId/nodeInstanceId, 동일 genesis, 다시 `RUNNING`/로컬 `READY` |
| 두 번째 정상 stop 및 setup 재개·finish | 종료 검증, 추가 엔진 attempt 없음, 최종 job `NOT_LOADED`·engine `STOPPED` |

실제 시험 root는 `/private/tmp/bxdl-setup-install-98posnt3`이며 `acceptance-result.json`과 private `evidence/`에 단계별 JSON·정제한 PTY 출력을 보존했다. 저장소에는 이 실행 파일·JAR/JRE·archive·키·DB·원시 실행 자료를 넣지 않았다. 재현 harness는 작업 폴더의 `acceptance-tools/smoke_setup.py`에 있다.

엔진 JAR SHA-256은 `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455`다. 기존 로컬 Oracle Java 21.0.7 jlink runtime을 시험에 재사용했으며 정식 배포 runtime 선정·재배포 승인을 의미하지 않는다. fixture의 consensus auto-start와 sync는 기존 `false`를 유지했다. 이번 실행은 단일 노드 관리 흐름 검증이며 거래 확정·4-validator quorum 인수가 아니다.

D 중 실제 package/TTY 정상 흐름·입력 취소/재개·공백 경로·같은 DB 재시작은 이번 CLI로 확인했다. 실제 프로세스 강제 중단의 각 checkpoint 경계와 실패 주입, signed 실제 package, `--from` 없는 전체 14문답, 로그아웃·sleep/wake 인수는 남았다. 단위·가짜 엔진 시험의 실패 경계를 실제 JVM 인수로 표시하지 않는다.

## 남은 범위

최신 NIGO의 일치하는 인계 묶음 수신·새 pin 및 재인수, 같은 Mac package의 4-validator 거래 확정·재시작, 전체 G1-M, 정식 JRE·최소 OS·라이선스/배포 자료, native/PKI 도우미, Linux/systemd·Docker와 기존 DB 업그레이드는 후속이다. 기본 setup의 install 모드를 지원한다고 이 항목들이 완료된 것은 아니다.
