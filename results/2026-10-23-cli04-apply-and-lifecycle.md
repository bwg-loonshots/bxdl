# CLI-04 설치 적용과 Mac RC.3 운용 인수

- 검증일: 2026-10-23
- 최종 판정: **PASS**
- 범위: macOS arm64, 기존 RC.3 패키지, validator 1개, `consensus.auto-start=false`, sync 비활성, peers 없음
- 최종 바이너리 검증 시작(UTC): `2026-10-23T06:11:46.501806+00:00`
- 상세 수치와 근거 해시: [JSON 결과](2026-10-23-cli04-apply-and-lifecycle.json)

## 빌드별 근거

| 회차 | 관리 CLI SHA-256 | 결과와 범위 |
|---|---|---|
| 최종 C release | `8449d8fa3e8f5fc84f7a8b30ba620291b6ea3af729dfa44049b887c53c5e735a` | 예약 namespace 출력 경로 보호 포함, 새로운 루트에서 전체 lifecycle **12개 PASS** |
| 이전 B release | `32ef19baa72624eb3ffd5a5dfd698b3f477dec22f6706326d7411a73e5a36e67` | core 12개 + 이름 충돌·준비 변경·읽기 전용 조회 3개, **15개 PASS** |
| 초기 A debug | `9fed946de173069ded3d1145ed2e3d33c1a7793bbb7a4153b766240cb7b304c0` | PTY harness 180초 제한으로 **INCOMPLETE**; 최종 인수 결과와 분리 |

B는 최종 예약 경로 보호 추가 전 바이너리다. B의 추가 사례를 최종 C에서 실행했다고 간주하지 않는다. 최종 C는 새 루트에서 설치부터 정상 종료·재적용까지 다시 검증했다. 최종 제품 소스 identity는 `4621366453357314dc710d6fb4f5c231ab12233b8e80452dd6f69b6ff393de75`이며, 비테스트 소스 및 Cargo/toolchain 파일 67개를 실행 시작·종료와 결과 작성 시 비교했다. 최종 바이너리도 같은 SHA-256을 유지했다. 전체 소스 identity는 `bb3d576b08a1791e868091b3e7b936e1c2df71434ca547d17b7e898ef2dc2f27`이다.

## 최종 C 검증

다음 명령을 별도의 임시 관리 홈과 새 데이터 경로에서 실행했다. 초기화와 시작은 각각 별도 명령과 승인으로 진행했다.

```sh
bxdl setup new local --from instance.json --home <전용-home>
bxdl setup apply local --home <전용-home>
bxdl init local --confirm-initialize --home <전용-home>
bxdl start local --home <전용-home>
bxdl status local --wait --home <전용-home>
bxdl stop local --home <전용-home>
bxdl setup apply local --home <전용-home>
```

| 사례 | 확인 결과 |
|---|---|
| 1. 순차 준비 | 기존 product/native fixture를 가져와 답을 저장하고 설정 파일만 생성 |
| 2. 준비 조회 | `setup list/show --json`이 관리 홈을 변경하지 않음 |
| 3. apply 거절 | `n` 응답 후 package/control/operations/DB 미생성, 준비 상태 보존 |
| 4. apply 승인 | `y` 응답 후 원본 RC.3 설치·cold 검사·V2 등록·`local` 이름 연결, DB 미생성 |
| 5. 설치 결과 조회 | 설치·등록 CONFIRMED, 이름 CONNECTED, 실행 상태 NOT_OBSERVED; 조회로 파일 변경 없음 |
| 6. 이름 기반 init | `--confirm-initialize`로 초기화 확인, 이 시점에 새 DB 생성 |
| 7. 이름 기반 start/status | 로컬 READY, 사유 `MANUAL_START_INITIALIZED` |
| 8. 이름 기반 stop | `SERVICE_STOPPED_VERIFIED`, 엔진 STOPPED·launchd job NOT_LOADED |
| 9. 완료 노드 재apply | `y` 재확인 후 재설치·재초기화 없음; package/control/DB 내용·inode·metadata 동일 |
| 10. 종료 후 조회 | `setup list/show`가 실행 기록을 읽고 관리 홈을 변경하지 않음 |
| 11. 종료 잔여 확인 | 기록 PID 19개 부재, launchd job 1개 NOT_LOADED, TCP listener 0개, 전용 루트 프로세스 0개 |
| 12. 불변성 | 공급 archive·key·lock·fixture와 설치 payload·JAR·JRE·원래 executor 불변 |

재적용 전후 package/control/DB의 파일 내용과 device/inode, 권한, 크기, mtime/ctime을 비교했다. 데이터 디렉터리 identity와 `engine-instance.json`도 유지했다. 조회 검증은 관리 홈 전체의 동일 항목을 비교했다. 조회 결과의 `runtime=NOT_OBSERVED`는 실행 상태를 조사하지 않았다는 뜻이다. 별도 `init` 결과를 설치 workflow의 과거 초기화 기록으로 덮어쓰지 않으며 다음 명령은 현재 control 기록을 확인해 제시한다.

## 이전 B의 추가 거절 사례

- 별도 관리 홈에 준비한 이름을 다른 control에 연결한 후 apply: `NODE_NAME_EXISTS`, 새 package/control/operations/DB 생성 없음, 기존 노드 보존.
- 정상 종료 후 준비의 `instanceId`만 편집한 다음 apply: `SETUP_PREPARATION_INCOMPLETE`, 기존 생성 설정과 설치된 package/control/DB 보존.
- 이 상태의 `setup list/show`: 변경된 준비를 부분 결과로 표시하면서 이전 등록 완료 기록을 유지하고 파일을 변경하지 않음.

## 같은 RC.3 공급물

| 항목 | SHA-256 |
|---|---|
| archive | `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017` |
| manifest | `2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123` |
| 원래 package executor | `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef` |
| NIGO JAR | `05a38bd1de45d16f0529a8689c113d6a359989ff0cd95e4c507bfc49843ee46f` |
| Java 실행 파일 | `9be1d0a740ff6502df1a762145e62860f5de4b7e17658d9cb9498da3acf9d16c` |

payload 270개, JRE 259개의 파일 membership·hash·size·mode를 확인했다. 서비스 worker 1개도 원래 package executor와 일치했다. 엔진·패키지 재빌드 및 새 자격 발급은 하지 않았다.

## 초기 debug trial

초기 A는 debug CLI의 반복 archive 검증 중 PTY harness의 180초 제한에 도달하여 `PTY_PROMPT_TIMEOUT_apply-approve`로 INCOMPLETE를 기록했다. 승인된 작업을 kill하거나 재시도하지 않았다. 이후 설치·등록 CONFIRMED와 이름 연결 기록, 소유 프로세스 자체 종료를 관측했다. 기록 PID 6개와 전용 루트 프로세스·TCP listener가 모두 없고 DB와 service plist는 생성되지 않았다. 이 회차는 lifecycle PASS 근거로 사용하지 않는다. release 회차는 PTY/명령 제한을 900초로 두었다. 세 회차 최종 통합 관측에서도 기록 PID 51개 부재, launchd job 2개 NOT_LOADED, 전용 TCP 포트 6개의 listener 0개, 전용 루트 프로세스 0개를 확인했다.

## 자동 검사

- `make check`: Rust **545개**, Python **50개**, fmt·Clippy 통과. 최종 관리 홈 출력 경계 보강 전의 전체 검사이며 당시 `prepare_cli`는 33개였다.
- 최종 출력 경계 보강 뒤 `cargo test --locked --test prepare_cli`: **34개 전체 통과**. 추가된 1개 테스트 안에서 예약 namespace·잘못된 저장 공간·Mac 대소문자/부모 경로 별칭 등 12사례의 승인 전 거절과 무쓰기를 확인했다. 중복을 제외한 Rust 검사는 **546개**다.
- 최종 fmt·Clippy, `make check-linux` 타입 검사, `make build` release 빌드를 통과했다. Linux 실행/서비스 인수는 포함하지 않는다.
- 승인 중 archive/catalog 교체 거절, 준비 폴더 유실 후 실행 기록 조회, 등록 성공 후 이름 저장 실패와 재apply 이름만 복구, 완료 등록의 원본 공급 자료 부재, UNKNOWN 부분 출력 보존과 소유 결과 재개, 이름 기반 init/resume-init 확인 플래그 및 고정 실행기 인자를 확인했다.

표준 검사와 최종 회귀 검사·Linux 검사·release 빌드의 로그 해시는 JSON의 `automatedChecks.logSha256`에 기록했다. 앞의 최종 C 실물 인수는 출력 경계 보강이 포함된 최종 release 바이너리로 수행했다.

## 검증 범위와 보관

단일 validator의 수동 시작 로컬 준비 상태를 확인했다. global quorum, 거래 확정, 로그아웃·sleep, 프로세스 crash 복구, Linux systemd 운영은 이번 Mac 인수 범위가 아니다. 표준 테스트와 Linux 컴파일 결과는 위 자동 검사 항목과 구분한다.

원본 CLI 출력·PTY 기록·fixture·자격 자료·경로 및 프로세스 식별자는 비공개 전용 임시 루트에 보존했다. 저장소에는 위 요약과 근거 해시만 포함한다. 기존 사용자 노드·이전 검증 DB는 변경하지 않았다.
