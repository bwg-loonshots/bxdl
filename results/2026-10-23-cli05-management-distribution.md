# CLI05-02 — 독립 Mac 관리 CLI 배포·업데이트 검증

- 날짜: 2026-10-23
- 상태: **CLI05-02 구현·검증 PASS — Rust590, Python60 PASS/2 SKIP, 실제 Mac signed13+unsigned13사례**. 검사 종류별 결과는 아래에서 구분한다.
- 범위: macOS arm64에서 CLI-only 공급물·사용자 설치·업데이트·복귀·PATH/zsh 연결·기존 자료 보존
- 설계: [독립 Mac 배포](../design/2026-10-23-management-cli-distribution.md), [사용 가이드](../docs/management-cli.md)
- 기계 판독 근거: [실제 인수 JSON](./2026-10-23-cli05-management-distribution.json)

## 소스와 배포물의 식별

CLI05-01 커밋 `84351233dd6a882299d001bc363bdeb40afeeabc` 위의 이번 미커밋 구현을 `cargo build --locked --release`로 두 번 빌드했다. 실제 서로 다른 정식 release commit은 아직 없으므로 `BXDL_REVISION`에 아래 **합성 40자리 revision**을 주입했다. A/B 모두 version은 `0.1.0-dev`다. 이 값은 Git에 존재하는 release commit을 주장하지 않으며 정식 배포물로 사용하지 않는다.

| 시험 CLI | 합성 revision | 실제 바이너리 SHA-256 |
| --- | --- | --- |
| A | `aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa` | `0c658435b106f2d4faab60c0175b6a70e13c320562df77a319d1ceae8a3e6e84` |
| B | `bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb` | `ac90e8523f5104f8ec6ecb5216774db62fb8a3a0762e8af0fa47cb6866781279` |

| 배포 archive | SHA-256 |
| --- | --- |
| A · signed | `0f84e7bafd23079491f3fc1114208138fd915675d8133700b3f4cf8785da2bce` |
| A · unsigned | `f36f7e1342acfeca9f8d8a50ffebe87448c13ac7acf47a0ba2d1e7c1e935cadf` |
| B · signed | `29a54e80752c6dae3eb9303e4ca90dbfff8abb8c1875d1d6c736027d8a3a824c` |
| B · unsigned | `59755c0d5a4af5f070a14a7df5a5fd52a6792c6af286ca94449714ea3a82ee7a` |

도구는 `rustc 1.86.0 (05f9846f8 2025-03-31)`, `cargo 1.86.0 (adf9b6ad1 2025-02-28)`다. 빌드 소스 fingerprint는 `67efd709c71b28040edb746d82ddb191d6ad6632feda2c6501e51bcafe1ff208`, 인수 하네스 hash는 `5dd75ca28df3138d659146441af80cfc7a442522135cfa4dd795e077933bf7fa`이며 계산 형식은 JSON에 기록한다. 파일 순서는 전체 경로 문자열 사전순이 아니라 Python `sorted(pathlib.Path 목록)`의 경로 구성요소 순서다. 각 상대 POSIX 경로 UTF-8·NUL·해당 파일 SHA-256 bytes를 이어 hash한다. 배포본에는 제작 당시 동봉 가이드 snapshot을 담았으며 이후의 결과/완료 상태 문서는 저장소에 기록한다.

signed run은 시험 중 새로 생성한 Ed25519 키를 사용했고 공개키 SHA-256은 `0c1ff28ba0e8b806e325692cf7641cda76a8d9f7eeb0e94abe5631aebf614105`다. 키는 공급 archive 밖에 두고 최초 설치에만 공개키를 지정했다. 시험 종료 뒤 임시 private key는 제거했다. 이후 update는 저장한 pin을 재사용했다. unsigned run은 별도의 임시 설치에서 설치·업데이트마다 `--allow-unsigned-development`를 지정했다. 두 신뢰 정책을 같은 root에서 전환하지 않았다. Apple code signing·notarization·공식 공급망 인수를 수행한 결과가 아니다.

초기 기능 인수 이후 사람용 안내와 저장 기록 보강을 반영해 다시 빌드했다. 아래 PASS와 JSON hash는 최종 A/B에만 해당하며 앞선 임시 빌드를 누적 집계하지 않는다.

인수 이후 [BXDL PR #18](https://github.com/bwg-loonshots/bxdl/pull/18)의 Linux CI가 CLI-04 테스트 파일의 Mac 전용 `Workflow` import를 unused import로 거절했다. 해당 import에 기존 사용 테스트와 같은 Mac arm64 조건을 추가했다. `src/setup/preparation/tests/application.rs`의 테스트 컴파일 조건만 바뀌었으며 실행 제품 코드·배포 payload는 변경하지 않았다. 위 소스 fingerprint는 실제 A/B 인수 당시의 테스트 소스까지 포함하는 역사적 값으로 보존한다. CI 보강 이후 공유 소스는 PR의 후속 커밋으로 식별하며 당시 A/B 바이너리를 다시 빌드한 것으로 기록하지 않는다.

## 자동 검사

| 검사 | 결과 |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --locked --all-targets -- -D warnings` | 최초 불필요한 unit 표현식 경고 수정 후 PASS |
| `cargo check --locked --target x86_64-unknown-linux-gnu` | PASS · Linux 실제 실행 아님 |
| Python 기본 discovery | 62개 중 60 PASS, 환경 opt-in 2 SKIP · 실제 인수 PASS로 계산하지 않음 |
| `cargo test --locked` 전체 Rust 회귀 | 16그룹 합계590 PASS · 0 FAIL/0 ignored |
| 최종 A/B Mac release 빌드 | PASS |
| 개발 builder script 실제 제작·umask077 설치 | PASS · 명시 unsigned development 배포의 별도 확인 |
| 별도 실제 Mac distribution 하네스 | signed 1회/13사례, unsigned 1회/13사례 PASS |

bundle 회귀는 고정 inventory·manifest·hash·서명·경로와 archive 한도를 확인한다. 저장소 회귀는 entry 소유·키 pin·동일 identity 충돌·손상·잠금·전환과 원자 게시를 확인한다. 후보 검사 회귀는 제한된 `version --json`·stdin 비사용·잘못된 응답·timeout·출력 한도를 검사한다. 명령/완성 회귀는 비TTY·JSON·정확한 도움말·잘못된 인자 거절·중첩 명령 후보를 확인한다.

## 실제 Mac 인수

[하네스](../tests/acceptance/test_management_distribution.py)는 모든 명령을 임시 HOME에서 실행한다. 기존 관리 home의 valid catalog와 sessions/operations, 별도 control/data/runtime-package의 private canary fixture를 먼저 만들고 매 단계마다 inode·mode·mtime/ctime·파일 hash를 비교한다. 실제 RC.3 package나 운영 DB를 재인수한 시험은 아니다. 일반 source checkout이나 Cargo 없이 제공된 바이너리를 설치하는 사용자 경로를 검사한다.

| 사례 | signed | unsigned |
| --- | --- | --- |
| 설치 전 `cli info`: NOT_INSTALLED, root/entry 폴더 미생성 | PASS | PASS |
| A의 `bin/bxdl cli install`에서 --from 생략·기본 경로·umask077·정확한 0755/0644·새 zsh/기존 이름 | PASS | PASS |
| 같은 A 재설치: CLI root 전체 snapshot 불변 | PASS | PASS |
| 고정 entry에서 B 업데이트·새 zsh fpath 자동로드·배포 _bxdl와 공개 스크립트 bytes 일치 | PASS | PASS |
| 설치 후 `cli info` 무쓰기 | PASS | PASS |
| 손상 archive 거절·B 선택과 root 보존 | PASS | PASS |
| unsigned로 신뢰 약화 / unsigned opt-in 누락 거절 | PASS | PASS |
| 설치 entry가 일반 파일로 바뀌면 update 거절·그 파일과 current 보존 | PASS | PASS |
| 안전하지 않은 CLI root 권한에서 update 거절·B 선택 보존 | PASS | PASS |
| 다른 파일이 있는 bin-dir의 bxdl을 덮어쓰지 않음 | PASS | PASS |
| 기존 노드 관리 home을 CLI root로 채택하지 않음 | PASS | PASS |
| B에서 A로 rollback·새 zsh의 버전/Tab 확인 | PASS | PASS |
| --root만 명시한 update가 기록된 bin-dir을 재사용 | PASS | PASS |

각 모드에서 기존 fixture snapshot이 모든 단계에 걸쳐 유지됐다. `.zshrc`/`.zprofile`을 만들지 않았고 시험용 HOME은 종료 후 제거했다. 새 zsh에서는 `command -v bxdl`, 실제 `version --json`, `bxdl cli inf[Tab]`, `bxdl status loc[Tab]`을 확인했다. source와 설치된 고정 fpath의 자동로드를 모두 사용했다. 실행한 노드 명령은 0개다.

## 재실행과 남은 범위

기본 Python discovery는 환경을 지정하지 않으면 이 인수 1개를 skip한다. 실제 Mac arm64에서 `BXDL_MANAGEMENT_BINARY`, 서로 다른 두 `BXDL_MANAGEMENT_BUNDLE_A/B`를 제공한다. signed 인수는 `BXDL_MANAGEMENT_PUBLIC_KEY`도 지정한다. 키가 없으면 하네스가 매번 명시 unsigned opt-in을 사용한다. `BXDL_MANAGEMENT_RESULT`는 성공 결과 JSON을 저장하고 `BXDL_MANAGEMENT_BUILD_PROVENANCE`로 시험 배포본의 출처를 기록한다.

```sh
python3 -B -m unittest discover -s tests/acceptance -p test_management_distribution.py -v
```

CLI05-03의 새 관리 CLI와 고정 RC.3 사이 실제 서비스·네트워크 인수, CLI05-04의 작성자가 아닌 사람의 과업은 **NOT_RUN**이다. Mac 로그아웃/재로그인, 안전 제거, 기존 runtime upgrade·migration, A7, Linux 실행, Mac Intel, `.pkg`/Homebrew와 정식 고객 배포 인수도 이 결과로 완료 처리하지 않는다. NIGO/JRE pin이나 NIGO 리포지터리를 변경하지 않았다.
