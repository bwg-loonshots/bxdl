# CLI05-01 zsh 자동완성 검증

- 검증일: 2026-10-23
- 최종 판정: **PASS — CLI05-01 구현·검증 완료**
- 범위: 관리 CLI의 `completion zsh`, 읽기 전용 후보 조회, macOS arm64 zsh 5.9의 실제 Tab 동작
- 기준 커밋: `2f1788804699b1809f51089ae26bd7516a0d56d6` 위 CLI05-01 작업 트리
- 실제 엔진·LaunchAgent·RC.3 서비스/네트워크 재인수: **NOT_RUN**
- 작성자가 아닌 사용자의 과업 검증: **NOT_RUN**
- 최종 release 실제 결과: [JSON 기록](./2026-10-23-cli05-zsh-completion.json)

## 구현 범위

`bxdl completion zsh`는 자동완성 스크립트만 stdout으로 출력한다. 비TTY를 허용하며 `--json` 혼합은 기존 JSON 오류 envelope와 사용법 오류로 거절한다. 정확한 `completion`/`completion zsh` 도움말은 `--help`, `-h`, `help <topic>`과 JSON 결과의 같은 안내를 제공한다.

스크립트는 입력 중인 명령줄과 커서 위치를 별도 내부 조회에 전달한다. 이 조회는 실제 명령 dispatch를 하지 않는다. 명령·옵션·정해진 값과 명시 `--home` 또는 기본 Mac 관리 위치의 저장 이름을 제안하며 파일/디렉터리 값은 zsh의 경로 완성으로 연결한다. 후보 문자열을 eval하거나 입력 중인 변수·명령 치환을 실행하지 않는다. 따옴표 없는 별도 인자의 `~/`만 HOME 경로로 안전하게 바꾼다.

노드 후보는 저장 catalog의 명시 alias만 읽는다. 대상 control·package·engine instanceId·표시명을 운용 이름으로 추정하지 않는다. `setup resume/edit/apply`는 private nonsymlink `sessions/<이름>` 디렉터리만, `setup show`는 같은 조건의 sessions/operations 이름 합집합을 제안한다. 준비 내용·공급 자료의 유효성이나 실행 가능성은 Tab에서 검사하지 않는다.

셸 설정·PATH·기존 CLI를 자동 수정하지 않는다. [사용 가이드](../docs/completion.md)에 현재 셸 source와 사용자가 선택하는 fpath 설치 방법을 설명한다. bash/fish는 이번 구현 범위가 아니다.

## 검사 회차

전체 검사와 이후 보강 검사를 구분한다. 아래 횟수를 더해 서로 다른 시험의 총개수로 표시하지 않는다. 마지막 release는 첫 `=` 분리 보정을 포함하며 그 바이너리로 실제 zsh 결합 시험을 다시 수행했다.

| 검사 | 결과 | 실제 범위 |
| --- | --- | --- |
| 전체 `make check` | **PASS · Rust561/Python58** | 당시 전체 회귀와 fmt·Clippy. 이후 보강은 아래 별도 회차에 기록 |
| 이후 Rust `completion_cli` | **15개 PASS** | group root의 지원하지 않는 도움말 후보 거절까지 포함한 최종 Rust 후보/조회 회귀 |
| 이후 실제 zsh 프런트엔드 | **10개 PASS, opt-in 실제 바이너리 시험 1개 skip** | provider stub을 사용한 실제 ZLE의 source/fpath·인용·커서·경로·리터럴·실패 처리·셸 옵션 보존 |
| 마지막 프런트엔드 보강 | **2개 PASS** | 첫 `=`만 옵션/값 구분자로 분리해 `data=1/...` 경로를 보존, 고정 옵션을 포함한 셸 alias 전달 |
| 최종 release 실제 zsh | **13시나리오 + 인용 바이너리 경로 PASS** | 실제 Rust provider와 공개 source 명령을 연결. 최종 바이너리 identity는 아래 표와 JSON에 기록 |
| 최종 fmt·Clippy | **PASS** | 경고를 오류로 취급한 Clippy 포함 |
| Linux 타입 검사 | **PASS** | `x86_64-unknown-linux-gnu` 타입 검사. Linux 실행/서비스 인수는 아님 |
| 최종 release 빌드·내장 스크립트 대조 | **PASS** | `make build`, `bin/bxdl completion zsh`와 `completions/_bxdl` 일치 |

## 최종 release의 실제 zsh 결합

`zsh 5.9 (arm64-apple-darwin25.0)`의 격리된 HOME/ZDOTDIR에서 `source <(bxdl completion zsh)`로 등록했다. Tab 뒤 편집 buffer를 확인하며 운용 명령을 Enter로 실행하지 않았다.

| 시나리오 | 확인한 동작 |
| --- | --- |
| `bxdl diagn` | 최상위 명령 완성 |
| `status --home <HOME> loc` | 명시 관리 위치의 노드 이름 |
| `status led --home <HOME>` | 커서 뒤에 있는 home도 선택에 반영 |
| `--json status --home <HOME> loc` | 입력 명령의 JSON 옵션과 내부 후보 프로토콜 구분 |
| `setup resume --home <HOME> al` | 준비 세션 이름 |
| `setup show --home <HOME> comp` | 별도 operations에 남은 이름 |
| `setup edit alpha --field=httpP` | 옵션 접두어를 보존한 field 값 완성 |
| `install pac` | archive 경로 완성 |
| `status --home=man` | inline 옵션의 디렉터리 값 완성 |
| `status --home=data=1/ne` | 값 안의 추가 `=`와 공백 경로 보존 |
| `status --home ~/management\ home loc` | 따옴표 없는 별도 `~/` 인자를 현재 HOME에 연결 |
| `status -- unknown` | 리터럴 경계 뒤에서 임의 파일로 fallback하지 않음 |
| `status --home <없는-home> loc` | 후보 없음, 관리 위치 미생성 |

공백이 포함된 바이너리 경로를 인용해 실행하는 경우도 확인했다. 관리 홈 전체의 파일 bytes·mode·inode·mtime/ctime이 전후 동일하고 private canary는 터미널에 나타나지 않았다. 실행한 노드 명령은 **0개**다. 임시 셸과 fixture는 시험 범위에서 정리했다.

## 회귀로 고정한 경계

- 공개 스크립트는 raw stdout만 출력하며 비TTY에서 stdin을 읽지 않는다. JSON 요청·미지원 셸·잘못된 provider 구조/크기/커서 위치를 거절한다.
- 후보 조회의 `--` 경계 안에 전달한 `--json`은 프로토콜 모드로 해석하지 않는다. 입력 명령줄의 실제 `--` 뒤 인자는 리터럴로 유지한다.
- 옵션값·inline `--name=value`·커서 뒤의 명시 `--home`·이미 입력한 옵션·상호 배타 조합을 구분한다. `--instance <경로>`로 대상을 정한 뒤의 빈 현재 단어는 옵션 완성으로 처리한다.
- 완료된 옵션값의 빈 문자열·잘못된 `--...`를 실제 CLI와 같이 거절한다. 하위 명령 후보도 뒤쪽 `--json`·옵션 제약을 대조하며 정확한 도움말 형식에서만 도움말 옵션을 제안한다.
- 없는/손상된 관리 위치·카탈로그, symlink/비공개 조건 위반 준비 항목, namespace의 256/257개 경계, 명시 이름 규칙을 확인한다. catalog의 표시명·대상 경로·비밀 canary를 후보로 출력하지 않는다.
- 후보 조회 전후 관리 홈의 파일 내용·inode·권한·mtime/ctime을 비교한다. 관리 root·lock·기록을 새로 쓰지 않으며 저장 대상 control·JAR/JRE·서비스·네트워크를 호출하지 않는다.
- 프런트엔드 회귀는 실패·잘못된 후보 출력·none의 파일 fallback 방지, single/double quotes·공백·alias·커서 prefix/suffix, 인용한 `$(...)`와 파일 helper의 리터럴 처리, 셸 함수 미호출과 로컬 옵션 복원을 확인한다.

## 바이너리와 로그 근거

| 항목 | SHA-256 |
| --- | --- |
| 최종 관리 CLI `bin/bxdl` | `ca9b794c28d757d565fa7fad7fe3827932bf4437bce0c96e5630ae9ead39e439` |
| 내장 zsh 스크립트 `completions/_bxdl` | `3e6c3092b6fde9bf3257a40e4765912a426326b97ea1321bcb247c0a0ea38d3d` |
| 전체 `make check` 로그 | `ca42a505f6a5ae44a237e8407fd0fabf57851c2b0f21c033f5024fc79d951ad5` |
| 최종 Clippy 로그 | `ccf989120fbe892cb30f9ebf03fbe3a2b18044c7464683a39bb395080a0af710` |
| Linux 타입 검사 로그 | `b81c8e2031cc7051387092e8cdf814bc4894b12dc93674ef8aa70b5cc87c78bb` |
| 최종 release 빌드 로그 | `2c00f73a3a1b4b8d8361d556b95290d3be13f455dbeddd7230747f37bc61ea32` |

원본 로그와 임시 실행 파일은 저장소에 포함하지 않는다. 실제 바이너리 결합 시험의 재실행은 명시 `BXDL_COMPLETION_BINARY`로 선택하며, 기본 acceptance의 해당 skip을 미실행 성공으로 세지 않는다. 이번에는 최종 바이너리로 별도 실행한 JSON의 PASS를 근거로 삼는다.

## 후속 범위

이번 결과는 CLI05-01만 다룬다. [구현 계획](../design/2026-10-22-integrated-operations-ux-implementation-plan.md)의 CLI05-02 독립 Mac 관리 CLI 배포, CLI05-03 관리 CLI×기존 runtime의 서비스·네트워크 조합 인수, CLI05-04 작성자가 아닌 사용자의 과업은 후속 PLAN이다. 사람 과업은 agent의 PTY 자동 시험으로 대체하지 않는다.

기존 RC.3 runtime package·실행기·NIGO JAR·JRE를 다시 만들거나 교체하지 않았다. 이 자동완성 구현에 새 NIGO 선행 API나 회신은 요구하지 않는다. 기존 서비스 인수 기록을 이번 변경 소스에서 재실행한 결과로 제시하지 않으며 전체 CLI-05·G1-M·고객 배포 완료를 주장하지 않는다.
