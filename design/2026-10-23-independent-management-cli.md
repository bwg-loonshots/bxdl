# 관리 CLI와 노드 실행기 분리 — 2026-10-23

구현·검증 완료: [최종 결과](../results/2026-10-23-independent-management-cli.md)의 직접 운용16사례·기존 설치 도우미 PTY8사례와 독립 대조가 PASS다.

## 결정과 범위

관리 CLI는 노드 패키지 밖에서 독립적으로 업데이트한다. 기존 RC.3 패키지의 `bin/bxdl`은 고정된 노드 실행기로 재사용한다. 패키지·NIGO JAR·JRE·서명·기존 control/data를 관리 CLI 업데이트 때문에 다시 만들지 않는다. 현재 source의 자체 실행 경로와 기존 패키지의 worker 검사는 보존한다.

첫 지원 대상은 [기존 RC.3 인수](../results/2026-10-22-macos-rc3-package.md)에서 확인한 정확한 패키지다. 이를 `BXDL_RC3_EXECUTOR_V1` 실행기 호환 profile로 관리한다. 같은 버전 문자열이나 같은 NIGO JAR만으로 다른 실행기를 허용하지 않는다. 새로운 profile은 명령 계약 검토와 조합 검증을 거쳐 추가한다.

## 구성 요소와 버전

| 구성 | 소속·저장 | 업데이트 조건 |
| --- | --- | --- |
| 관리 CLI | BXDL 저장소에서 빌드, 노드 설치본 밖에 보관 | 이름·도움말·문답·표시·호환 adapter 변경 |
| 노드 실행기 | 기존 설치 package의 검증된 `bin/bxdl` | 노드 실행·기록·보안 처리 변경 시 package 재배포 |
| NIGO 엔진/JRE | 동일 package의 고정 JAR/runtime | 엔진·런타임 변경 시 package 재배포·인수 |
| 관리 목록 | 사용자 관리 home/catalog | 이름 연결 등 명시 목록 작업 |
| 노드 제어·원장 | 기존 control 및 data 경로 | 실행기가 기존 계약에 따라 갱신 |

`bxdl version --json`의 `version`/`revision`은 호출한 관리 CLI를 식별한다. 관리 CLI의 Cargo 버전·release 주기는 NIGO release tag와 맞추지 않는다. 엔진/JRE와 package 실행기의 identity는 기존 manifest·engine lock과 실행기 profile로 고정한다. 기존 관리 CLI와 실행기에 같은 `0.1.0-dev` 문자열이 있을 수 있으므로 호환 판정에는 문자열 대신 정확한 hash를 사용한다.

관리 CLI는 상주 웹서버나 원격 제어 데몬을 추가하지 않는다. 사용자 LaunchAgent와 Java 프로세스 수명은 기존과 같다. 관리 CLI 버전·실행기 profile·engine/JRE identity를 별개로 다루며 archive/inventory hash 검증을 유지한다.

## 실행 경로

```text
관리 CLI
  → 이름 또는 --instance로 기존 control 선택
  → 지원 profile 식별 및 패키지/실행기 인증
  → 선택 대상 재검사
  → 고정 argv와 --json으로 package/bin/bxdl 실행
  → 검증한 결과를 사람용/JSON으로 표시

package/bin/bxdl
  → 기존 operation lock·durable journal·입력 검증
  → 기존 worker snapshot·LaunchAgent·NIGO 실행
```

지원 명령은 `instance show`, 등록형 `preflight`, 명시 `init`/`resume-init`, `start`, `status`/`status --wait`, `stop`, `logs`, `diagnose`다. 확인 플래그·한도는 관리 CLI에서 먼저 검사하고 실행기에 고정 인자로 전달한다. `service-run`·임의 명령·임의 바이너리·PATH 탐색·셸 문자열은 위임 API로 노출하지 않는다.

`status/start/stop/logs/diagnose <이름> [--home <root>]`도 연결한다. 이름과 `--instance`는 배타적이며 경로 명령에 `--home`을 섞지 않는다. 대상 누락·알 수 없는 이름은 추정하지 않는다. 목록의 cached 상태를 실행 승인으로 사용하지 않는다.

설치와 최초 등록은 기존 BXDL installer/등록 API를 유지한다. 호환되는 기존 RC.3의 V1/V2 binding 계약에 따라 등록하고, 등록 이후의 실제 노드 작업은 고정 실행기에 위임한다. 기존 `setup --install`의 package 선택 검사는 정확한 지원 profile이면 관리 CLI bytes 불일치를 허용한다. 문답의 승인·workflow 소유 증거·UNKNOWN 보존은 그대로 유지하며 init/start/status/wait만 공통 실행기 경로에 연결한다. 순차 `setup new/resume` UX는 CLI-03에서 구현했다. CLI-04의 `setup apply`는 별도 승인으로 기존 설치·등록 API를 호출하고 완료 후 이름을 연결한다. 이름 기반 init/resume-init도 기존 고정 실행기 요청을 사용하며 준비/실행 기록과 초기화·start 승인을 분리한다. 새 경로의 최종 검증은 CLI-04에서 별도로 기록한다.

## 인증·호환 계약

`BXDL_RC3_EXECUTOR_V1`은 다음 기존 identity로 한정한다.

- archive SHA-256: `8a45cb8bf3482333ad43e8c29ad24c2adde8d6fe778eaf27fe2f5c2c842eb017`
- manifest SHA-256: `2647e9bfb65be83a532a4f37523dcc9ab54492e609d10a646cd0bf535109d123`
- executor SHA-256: `b7d677b44de30275315ef4a7ccf1e4230b66034d1964c27cbbd3bc586c622cef`, 3,024,064 bytes
- engine/JRE/contract identity: 위 archive와 기존 engine lock의 고정 값

유효한 binding의 profile 식별 후에는 archive 서명·전체 inventory·설치 receipt·실행기 hash·engine lock 관계를 확인한다. control/root/binding과 실행기 파일은 실행 직전에 다시 확인한다. stop·오프라인 진단은 변경되거나 사라진 credential 때문에 차단하지 않도록, engine 입력 pin 검사는 기존 실행기의 명령별 정책에 맡긴다.

알려진 RC.3 binding의 인증 실패는 명시 오류이며 다른 실행 방식으로 우회하지 않는다. 지원되지 않는 package와 식별할 수 없는 손상 control은 외부 실행 없이 기존 native 동작을 유지한다. 이는 임의 구버전 실행기 허용이 아니며 start의 기존 exact CLI 조건과 손상 자료에 대한 진단 partial 계약을 보존하기 위한 구분이다.

기존 실행기의 `SERVICE_CLI_MISMATCH` 검사는 제거하지 않는다. 실제 child가 package 안의 동일 CLI이므로 기존 검사를 통과한다. 새 관리 CLI가 구형 private runtime journal을 직접 작성하지 않는다. 인증한 executable을 현재 설치본에서 실행하며 같은 UID의 악의적 동시 파일 교체에 대한 강한 보안 격리는 기존과 같이 제공하지 않는다.

## 결과·실패·중단

- 표준입력은 닫고 shell 없이 명시 argv로 호출한다. 필요한 최소 실행 환경만 전달한다.
- stdout/stderr를 제한된 양만 수집한다. stderr 원문이나 잘못된 JSON의 원문을 사용자에게 되돌리지 않는다.
- JSON 문서 하나·envelope schema·요청 command·outcome/exit 관계와 명령별 필수 data를 검사한다. READY/partial/UNKNOWN과 child 관측 시각을 보존한다.
- 실행 전 인증/인자 오류와 child 실행 후 결과 불명을 구분한다. 실행 후 잘못된 출력·기한 초과·결과 유실을 미실행으로 표시하지 않는다. 변경 가능 작업은 UNKNOWN/exit 6으로 남기며 자동 재시도하지 않는다.
- 관리 CLI의 감독 시간 제한은 child의 명령 timeout보다 여유 있게 설정한다. 감독 실패 때문에 노드나 프로세스그룹에 강제 종료를 보내지 않는다. child 회수와 출력 수집에는 상한을 두고 노드 생존 여부는 별도 상태로 확인한다.
- 사용자가 관리 CLI를 중단해도 노드 작업 취소를 보장하지 않는다. control과 실제 상태를 다시 확인한다. 현재 legacy 실행기에는 expected binding/attempt 인자 계약이 없으므로 선택 검사와 child 잠금 사이를 하나의 원자적 승인으로 만드는 기능은 제공하지 않는다.
- 이름 대상은 위임 직전에 root/binding/catalog revision을 재검사한다. 구형 stop은 자신이 잠금을 획득한 시점의 현재 시도를 중지하는 기존 계약이다. 메뉴에서 오래 기다린 승인을 새로운 시도로 전이하는 기능은 추가하지 않는다.
- workflow의 wait는 기대한 attempt를 위임 전후에 대조한다. 다른 시도의 관측을 기존 시작의 완료로 채택하지 않는다.

## 단계별 구현·검증

1. **인증 adapter:** 명시 profile, 패키지 인증, fixed request, bounded child/결과 parser. 변조·미지원·불완전 출력·UNKNOWN·입력 미소비 회귀.
2. **직접 명령 연결:** 기존 경로와 이름 기반 운용, 같은 request/결과 모델, 손상 catalog의 명시 경로 fallback.
3. **기존 설치 문답 연결:** RC.3 선택 허용, installer/등록 소유 계약 유지, 노드 작업 위임. 이름/메뉴 UX 확대는 별도.
4. **동일 RC.3 실인수:** 기존 archive를 재빌드/재서명하지 않고 격리 설치. 서로 다른 관리 CLI 두 빌드로 초기화·시작·관측·정상 종료·같은 DB 재시작. 모든 worker가 기존 executor bytes인지 확인.
5. **독립 검사:** archive·package 270파일·JRE 259파일 불변, 고정 공급 자료, 정상 종료·소유 job/PID·시험 포트 정리. 코드 검사·Mac PTY·실제 package 인수를 구분 기록.

관리 기능만 바꿀 때는 새 관리 CLI와 지원 runtime 조합을 검증한다. 실행기·NIGO/JRE를 바꿀 때는 새 runtime package를 인수한다. 기존 RC.3 인수 기록은 이 변경의 PASS로 소급 사용하지 않는다. NIGO 저장소 변경은 이번 단계의 선행 조건이 아니다.
